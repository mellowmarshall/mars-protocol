#!/usr/bin/env bash
# ============================================================================
# Mesh Hub Fleet — Provider-Agnostic Multi-Region Deployment
# ============================================================================
# Deploys a mesh-hub fleet to any supported cloud provider.
#
# Usage:
#   ./deploy/fleet-deploy.sh <provider> [command]
#
# Providers:
#   hetzner       — ~$4.50/mo per node (cheapest, requires ID verification)
#   vultr         — ~$6/mo per node (no KYC, 32 locations)
#   digitalocean  — ~$6/mo per node (no KYC, popular)
#
# Commands:
#   deploy        — Create servers and deploy hubs (default)
#   destroy       — Tear down all servers
#   status        — Show fleet status
#   update        — Upload new binary and restart all hubs
#
# Examples:
#   ./deploy/fleet-deploy.sh hetzner
#   ./deploy/fleet-deploy.sh vultr deploy
#   ./deploy/fleet-deploy.sh digitalocean status
#   ./deploy/fleet-deploy.sh hetzner destroy
#
# Environment variables:
#   MESH_REGIONS      — Space-separated list of regions (default: "us-east us-west eu-central ap-southeast")
#   MESH_SERVER_TYPE  — Override server type/plan
#   MESH_IMAGE        — Override OS image
#   MESH_SSH_KEY      — Override SSH key name/ID

set -euo pipefail

# ============================================================================
# Arguments
# ============================================================================

PROVIDER="${1:-}"
COMMAND="${2:-deploy}"

if [ -z "$PROVIDER" ]; then
    echo "Usage: fleet-deploy.sh <provider> [deploy|destroy|status|update]"
    echo ""
    echo "Providers:"
    echo "  hetzner       ~\$4.50/node/mo  (ID verification required)"
    echo "  vultr         ~\$6/node/mo     (credit card only)"
    echo "  digitalocean  ~\$6/node/mo     (credit card only)"
    echo ""
    echo "Examples:"
    echo "  fleet-deploy.sh vultr                    # deploy 4-region fleet"
    echo "  fleet-deploy.sh vultr status             # check fleet health"
    echo "  fleet-deploy.sh vultr update             # push new binary"
    echo "  fleet-deploy.sh vultr destroy             # tear everything down"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$PROJECT_DIR/target/release/mesh-hub"

# ── Load provider ────────────────────────────────────────────────────────────

PROVIDER_FILE="$SCRIPT_DIR/providers/$PROVIDER.sh"
if [ ! -f "$PROVIDER_FILE" ]; then
    echo "ERROR: Unknown provider '$PROVIDER'"
    echo "Available providers:"
    for f in "$SCRIPT_DIR"/providers/*.sh; do
        echo "  $(basename "$f" .sh)"
    done
    exit 1
fi

# shellcheck source=/dev/null
source "$PROVIDER_FILE"

# ── Regions ──────────────────────────────────────────────────────────────────

IFS=' ' read -r -a DEPLOY_ORDER <<< "${MESH_REGIONS:-us-east us-west eu-central ap-southeast}"

# ============================================================================
# Shared functions
# ============================================================================

server_name() {
    echo "mesh-hub-$1"
}

wait_for_ssh() {
    local ip="$1"
    local attempts=0
    echo -n "  Waiting for SSH..."
    while ! ssh -o ConnectTimeout=2 -o StrictHostKeyChecking=accept-new "$PROVIDER_SSH_USER@$ip" true 2>/dev/null; do
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

ensure_binary() {
    if [ ! -f "$BINARY" ]; then
        echo "Building mesh-hub..."
        cd "$PROJECT_DIR"
        cargo build --release --bin mesh-hub
    fi
    echo "  Binary: $(du -h "$BINARY" | cut -f1)"
}

deploy_to_server() {
    local ip="$1" region="$2" seed_addr="$3"
    local name
    name=$(server_name "$region")

    echo ""
    echo "--- $region ($name @ $ip) ---"

    wait_for_ssh "$ip"

    echo "  Uploading binary..."
    scp -q "$BINARY" "$PROVIDER_SSH_USER@$ip:/usr/local/bin/mesh-hub"

    echo "  Uploading deploy scripts..."
    ssh -q "$PROVIDER_SSH_USER@$ip" "mkdir -p ~/deploy"
    scp -q "$SCRIPT_DIR/setup-node.sh" "$SCRIPT_DIR/mesh-hub.service" "$PROVIDER_SSH_USER@$ip:~/deploy/"

    echo "  Running setup..."
    if [ -z "$seed_addr" ]; then
        ssh -t "$PROVIDER_SSH_USER@$ip" "bash ~/deploy/setup-node.sh $region" 2>&1 | sed 's/^/  /'
    else
        ssh -t "$PROVIDER_SSH_USER@$ip" "bash ~/deploy/setup-node.sh $region $seed_addr" 2>&1 | sed 's/^/  /'
    fi
}

update_allowlists() {
    local all_addrs=()
    local region name ip

    echo ""
    echo "=== Updating Peer Allowlists ==="

    for region in "${DEPLOY_ORDER[@]}"; do
        name=$(server_name "$region")
        ip=$(provider_get_server_ip "$name")
        all_addrs+=("$ip:4433")
    done

    local allowlist_toml
    allowlist_toml=$(printf ', "%s"' "${all_addrs[@]}")
    allowlist_toml="[${allowlist_toml:2}]"

    for region in "${DEPLOY_ORDER[@]}"; do
        name=$(server_name "$region")
        ip=$(provider_get_server_ip "$name")
        echo "  Updating $region ($ip)..."

        ssh -q "$PROVIDER_SSH_USER@$ip" "
            if grep -q 'outbound_allowlist' /etc/mesh-hub/mesh-hub.toml; then
                sed -i 's|outbound_allowlist = .*|outbound_allowlist = $allowlist_toml|' /etc/mesh-hub/mesh-hub.toml
            else
                sed -i '/\[security\]/a outbound_allowlist = $allowlist_toml' /etc/mesh-hub/mesh-hub.toml
            fi
            systemctl restart mesh-hub
        "
    done
}

print_status() {
    local first_ip="" region name ip status

    echo ""
    echo "=============================================="
    echo "  Mesh Hub Fleet — $PROVIDER_NAME"
    echo "=============================================="
    echo ""
    printf "  %-16s %-22s %s\n" "Region" "Address" "Status"
    printf "  %-16s %-22s %s\n" "──────────────" "────────────────────" "──────"

    for region in "${DEPLOY_ORDER[@]}"; do
        name=$(server_name "$region")
        ip=$(provider_get_server_ip "$name")
        if [ -z "$ip" ]; then
            status="not found"
        else
            status=$(ssh -q -o ConnectTimeout=3 "$PROVIDER_SSH_USER@$ip" "systemctl is-active mesh-hub" 2>/dev/null || echo "unreachable")
            [ -z "$first_ip" ] && first_ip="$ip"
        fi
        printf "  %-16s %-22s %s\n" "$region" "${ip:--}:4433" "$status"
    done

    echo ""
    echo "  Peering: enabled (gossip every 60s)"
    if [ -n "$first_ip" ]; then
        echo ""
        echo "  Connect an agent:"
        echo "    mesh-node publish --type 'compute/inference/text-generation' \\"
        echo "      --endpoint 'https://my-agent.example.com/v1/generate' \\"
        echo "      --seed $first_ip:4433"
    fi
    echo ""
}

# ============================================================================
# Commands
# ============================================================================

cmd_deploy() {
    echo "=== Mesh Hub Fleet Deploy ($PROVIDER_NAME) ==="
    echo ""

    provider_preflight
    ensure_binary

    echo ""
    echo "  Regions: ${DEPLOY_ORDER[*]}"
    echo ""

    # Create servers
    echo "=== Creating Servers ==="
    local region name ip
    for region in "${DEPLOY_ORDER[@]}"; do
        name=$(server_name "$region")

        if provider_server_exists "$name"; then
            ip=$(provider_get_server_ip "$name")
            echo "  $name already exists ($ip), skipping"
        else
            echo "  Creating $name in $region..."
            ip=$(provider_create_server "$name" "$region")
            echo "  Created: $ip"
        fi
    done

    # Deploy to each server
    echo ""
    echo "=== Deploying Hubs ==="

    local seed_addr=""
    for region in "${DEPLOY_ORDER[@]}"; do
        name=$(server_name "$region")
        ip=$(provider_get_server_ip "$name")
        deploy_to_server "$ip" "$region" "$seed_addr"
        [ -z "$seed_addr" ] && seed_addr="$ip:4433"
    done

    update_allowlists
    print_status
}

cmd_destroy() {
    echo "=== Destroying Mesh Hub Fleet ($PROVIDER_NAME) ==="
    local region name
    for region in "${DEPLOY_ORDER[@]}"; do
        name=$(server_name "$region")
        if provider_server_exists "$name"; then
            echo "  Deleting $name..."
            provider_delete_server "$name"
        else
            echo "  $name not found, skipping"
        fi
    done
    echo "Done."
}

cmd_status() {
    provider_preflight
    print_status
}

cmd_update() {
    echo "=== Updating Mesh Hub Fleet ($PROVIDER_NAME) ==="
    provider_preflight
    ensure_binary

    local region name ip
    for region in "${DEPLOY_ORDER[@]}"; do
        name=$(server_name "$region")
        ip=$(provider_get_server_ip "$name")

        if [ -z "$ip" ]; then
            echo "  $name not found, skipping"
            continue
        fi

        echo "  Updating $region ($ip)..."
        wait_for_ssh "$ip"
        scp -q "$BINARY" "$PROVIDER_SSH_USER@$ip:/usr/local/bin/mesh-hub"
        ssh -q "$PROVIDER_SSH_USER@$ip" "systemctl restart mesh-hub"
        echo "  Restarted"
    done

    print_status
}

# ── Dispatch ─────────────────────────────────────────────────────────────────

case "$COMMAND" in
    deploy)  cmd_deploy  ;;
    destroy) cmd_destroy ;;
    status)  cmd_status  ;;
    update)  cmd_update  ;;
    *)
        echo "Unknown command: $COMMAND"
        echo "Commands: deploy, destroy, status, update"
        exit 1
        ;;
esac
