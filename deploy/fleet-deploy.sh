#!/usr/bin/env bash
# ============================================================================
# Mesh Hub Fleet — Multi-Region Deployment
# ============================================================================
# Deploys mesh-hub to multiple Hetzner VPS instances from your local machine.
#
# Prerequisites:
#   1. Create 4 Hetzner CX22 VPSes (Debian 12) in each region
#   2. Add your SSH key to each
#   3. Fill in the IP addresses below
#   4. Run: ./deploy/fleet-deploy.sh
#
# What it does:
#   1. Cross-compiles mesh-hub for x86_64-linux (or uses local build)
#   2. Uploads binary + deploy scripts to each VPS
#   3. Runs setup-hetzner.sh on each, chaining seed addresses
#
# Cost: ~$17/mo total for worldwide coverage

set -euo pipefail

# ============================================================================
# CONFIGURE THESE — fill in your Hetzner VPS IPs
# ============================================================================

US_EAST_IP=""          # Ashburn, VA
US_WEST_IP=""          # Hillsboro, OR
EU_CENTRAL_IP=""       # Falkenstein, DE
AP_SOUTHEAST_IP=""     # Singapore

# SSH user (root for fresh Hetzner VPSes)
SSH_USER="root"

# ============================================================================

declare -A HUBS=(
    ["us-east"]="$US_EAST_IP"
    ["us-west"]="$US_WEST_IP"
    ["eu-central"]="$EU_CENTRAL_IP"
    ["ap-southeast"]="$AP_SOUTHEAST_IP"
)

# Deployment order — us-east is the seed, others chain from it
DEPLOY_ORDER=("us-east" "us-west" "eu-central" "ap-southeast")

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$PROJECT_DIR/target/release/mesh-hub"

# ── Validate ─────────────────────────────────────────────────────────────────

for region in "${DEPLOY_ORDER[@]}"; do
    ip="${HUBS[$region]}"
    if [ -z "$ip" ]; then
        echo "ERROR: No IP configured for $region"
        echo "Edit this script and fill in the IP addresses."
        exit 1
    fi
done

# ── Build ────────────────────────────────────────────────────────────────────

echo "=== Building mesh-hub ==="

if [ ! -f "$BINARY" ]; then
    echo "Building release binary..."
    cd "$PROJECT_DIR"
    cargo build --release --bin mesh-hub
fi

echo "  Binary: $BINARY ($(du -h "$BINARY" | cut -f1))"

# ── Deploy ───────────────────────────────────────────────────────────────────

SEED_ADDR=""

for region in "${DEPLOY_ORDER[@]}"; do
    ip="${HUBS[$region]}"
    echo ""
    echo "=== Deploying $region ($ip) ==="

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
        # First hub — no seed
        ssh -t "$SSH_USER@$ip" "bash ~/deploy/setup-hetzner.sh $region"
        SEED_ADDR="$ip:4433"
        echo "  Seed address for remaining hubs: $SEED_ADDR"
    else
        # Subsequent hubs — seed from us-east
        ssh -t "$SSH_USER@$ip" "bash ~/deploy/setup-hetzner.sh $region $SEED_ADDR"
    fi

    echo "  $region deployed at $ip:4433"
done

# ── Summary ──────────────────────────────────────────────────────────────────

echo ""
echo "=============================================="
echo "  Fleet Deployment Complete"
echo "=============================================="
echo ""
echo "  Hub Addresses:"
for region in "${DEPLOY_ORDER[@]}"; do
    ip="${HUBS[$region]}"
    printf "    %-14s %s:4433\n" "$region" "$ip"
done
echo ""
echo "  Peering: enabled (gossip every 60s)"
echo "  Seed:    ${HUBS[us-east]}:4433"
echo ""
echo "  Check hub health:"
for region in "${DEPLOY_ORDER[@]}"; do
    ip="${HUBS[$region]}"
    echo "    ssh $SSH_USER@$ip systemctl status mesh-hub"
done
echo ""
echo "  Watch logs:"
echo "    ssh $SSH_USER@${HUBS[us-east]} journalctl -u mesh-hub -f"
echo ""
echo "  IMPORTANT: Save the operator tokens printed during each setup!"
echo ""
echo "  Agents can now connect to any hub:"
echo "    mesh-node publish --type 'compute/inference/text-generation' \\"
echo "      --endpoint 'https://my-agent.example.com/v1/generate' \\"
echo "      --seed ${HUBS[us-east]}:4433"
echo ""
