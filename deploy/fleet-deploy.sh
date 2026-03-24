#!/usr/bin/env bash
# ============================================================================
# Mesh Hub Fleet — Multi-Region Deployment via Hetzner Cloud CLI
# ============================================================================
# Fully automated: creates VPSes, uploads binary, configures peering.
#
# Prerequisites:
#   1. Install hcloud CLI: brew install hcloud  (or https://github.com/hetznercloud/cli)
#   2. Create API token: Hetzner Cloud Console → Project → Security → API Tokens
#   3. Configure:  hcloud context create mesh-protocol
#   4. Add SSH key: hcloud ssh-key create --name mesh-deploy --public-key-from-file ~/.ssh/id_ed25519.pub
#   5. Build:      cargo build --release --bin mesh-hub
#   6. Run:        ./deploy/fleet-deploy.sh
#
# Cost: ~$17/mo total for 4 regions worldwide
#
# To tear down:
#   ./deploy/fleet-deploy.sh destroy

set -euo pipefail

# ============================================================================
# Configuration
# ============================================================================

SSH_KEY_NAME="mesh-deploy"       # Name of your hcloud SSH key
SERVER_TYPE="cx22"               # 2 vCPU, 4GB RAM, 40GB disk (~$4.50/mo)
IMAGE="debian-12"
SSH_USER="root"

declare -A REGIONS=(
    ["us-east"]="ash"
    ["us-west"]="hil"
    ["eu-central"]="fsn1"
    ["ap-southeast"]="sin"
)

# Deployment order — us-east is the seed, others chain from it
DEPLOY_ORDER=("us-east" "us-west" "eu-central" "ap-southeast")

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$PROJECT_DIR/target/release/mesh-hub"

# ============================================================================
# Functions
# ============================================================================

server_name() {
    echo "mesh-hub-$1"
}

get_server_ip() {
    local name
    name=$(server_name "$1")
    hcloud server ip "$name" 2>/dev/null || echo ""
}

wait_for_ssh() {
    local ip="$1"
    local attempts=0
    echo -n "  Waiting for SSH..."
    while ! ssh -o ConnectTimeout=2 -o StrictHostKeyChecking=accept-new "$SSH_USER@$ip" true 2>/dev/null; do
        attempts=$((attempts + 1))
        if [ $attempts -ge 30 ]; then
            echo " TIMEOUT"
            echo "ERROR: Could not connect to $ip after 60 seconds"
            exit 1
        fi
        echo -n "."
        sleep 2
    done
    echo " ready"
}

# ============================================================================
# Destroy mode
# ============================================================================

if [ "${1:-}" = "destroy" ]; then
    echo "=== Destroying Mesh Hub Fleet ==="
    for region in "${DEPLOY_ORDER[@]}"; do
        name=$(server_name "$region")
        if hcloud server describe "$name" &>/dev/null; then
            echo "  Deleting $name..."
            hcloud server delete "$name"
        else
            echo "  $name not found, skipping"
        fi
    done
    echo "Done."
    exit 0
fi

# ============================================================================
# Preflight checks
# ============================================================================

echo "=== Mesh Hub Fleet Deploy ==="

if ! command -v hcloud &>/dev/null; then
    echo "ERROR: hcloud CLI not found"
    echo "Install: brew install hcloud"
    echo "   or:   https://github.com/hetznercloud/cli"
    exit 1
fi

if ! hcloud ssh-key describe "$SSH_KEY_NAME" &>/dev/null; then
    echo "ERROR: SSH key '$SSH_KEY_NAME' not found in Hetzner Cloud"
    echo "Add it: hcloud ssh-key create --name $SSH_KEY_NAME --public-key-from-file ~/.ssh/id_ed25519.pub"
    exit 1
fi

if [ ! -f "$BINARY" ]; then
    echo "Building mesh-hub..."
    cd "$PROJECT_DIR"
    cargo build --release --bin mesh-hub
fi

echo "  Binary: $(du -h "$BINARY" | cut -f1)"
echo "  SSH key: $SSH_KEY_NAME"
echo "  Server type: $SERVER_TYPE ($IMAGE)"
echo ""

# ============================================================================
# Create servers
# ============================================================================

echo "=== Creating Servers ==="

for region in "${DEPLOY_ORDER[@]}"; do
    name=$(server_name "$region")
    location="${REGIONS[$region]}"

    if hcloud server describe "$name" &>/dev/null; then
        ip=$(get_server_ip "$region")
        echo "  $name already exists ($ip), skipping creation"
    else
        echo "  Creating $name in $location..."
        hcloud server create \
            --name "$name" \
            --type "$SERVER_TYPE" \
            --image "$IMAGE" \
            --location "$location" \
            --ssh-key "$SSH_KEY_NAME" \
            --label "role=mesh-hub" \
            --label "region=$region"

        ip=$(get_server_ip "$region")
        echo "  Created: $ip"
    fi
done

echo ""

# ============================================================================
# Deploy to each server
# ============================================================================

echo "=== Deploying Hubs ==="

SEED_ADDR=""

for region in "${DEPLOY_ORDER[@]}"; do
    ip=$(get_server_ip "$region")
    name=$(server_name "$region")

    echo ""
    echo "--- $region ($name @ $ip) ---"

    wait_for_ssh "$ip"

    # Upload binary
    echo "  Uploading binary..."
    scp -q "$BINARY" "$SSH_USER@$ip:/usr/local/bin/mesh-hub"

    # Upload deploy scripts
    echo "  Uploading deploy scripts..."
    ssh -q "$SSH_USER@$ip" "mkdir -p ~/deploy"
    scp -q "$SCRIPT_DIR/setup-hetzner.sh" "$SCRIPT_DIR/mesh-hub.service" "$SSH_USER@$ip:~/deploy/"

    # Run setup
    echo "  Running setup..."
    if [ -z "$SEED_ADDR" ]; then
        ssh -t "$SSH_USER@$ip" "bash ~/deploy/setup-hetzner.sh $region" 2>&1 | sed 's/^/  /'
        SEED_ADDR="$ip:4433"
    else
        ssh -t "$SSH_USER@$ip" "bash ~/deploy/setup-hetzner.sh $region $SEED_ADDR" 2>&1 | sed 's/^/  /'
    fi
done

# ============================================================================
# Update SSRF allowlists — each hub needs to allow connections to all peers
# ============================================================================

echo ""
echo "=== Updating Peer Allowlists ==="

# Collect all hub addresses
ALL_ADDRS=()
for region in "${DEPLOY_ORDER[@]}"; do
    ip=$(get_server_ip "$region")
    ALL_ADDRS+=("$ip:4433")
done

ALLOWLIST_TOML=$(printf ', "%s"' "${ALL_ADDRS[@]}")
ALLOWLIST_TOML="[${ALLOWLIST_TOML:2}]"  # trim leading ", "

for region in "${DEPLOY_ORDER[@]}"; do
    ip=$(get_server_ip "$region")
    echo "  Updating $region ($ip)..."

    # Replace or insert outbound_allowlist in the config
    ssh -q "$SSH_USER@$ip" "
        if grep -q 'outbound_allowlist' /etc/mesh-hub/mesh-hub.toml; then
            sed -i 's|outbound_allowlist = .*|outbound_allowlist = $ALLOWLIST_TOML|' /etc/mesh-hub/mesh-hub.toml
        else
            sed -i '/\[security\]/a outbound_allowlist = $ALLOWLIST_TOML' /etc/mesh-hub/mesh-hub.toml
        fi
        systemctl restart mesh-hub
    "
done

# ============================================================================
# Summary
# ============================================================================

echo ""
echo "=============================================="
echo "  Mesh Hub Fleet — Online"
echo "=============================================="
echo ""
echo "  Region           Address              Status"
echo "  ─────────────    ──────────────────    ──────"
for region in "${DEPLOY_ORDER[@]}"; do
    ip=$(get_server_ip "$region")
    status=$(ssh -q "$SSH_USER@$ip" "systemctl is-active mesh-hub" 2>/dev/null || echo "unknown")
    printf "  %-16s %-20s %s\n" "$region" "$ip:4433" "$status"
done
echo ""
echo "  Peering:   enabled (gossip every 60s)"
echo "  Seed hub:  $(get_server_ip us-east):4433"
echo ""
echo "  Connect an agent:"
echo "    mesh-node publish --type 'compute/inference/text-generation' \\"
echo "      --endpoint 'https://my-agent.example.com/v1/generate' \\"
echo "      --seed $(get_server_ip us-east):4433"
echo ""
echo "  Monthly cost: ~\$17 (4x CX22)"
echo ""
echo "  Tear down: ./deploy/fleet-deploy.sh destroy"
echo ""
