#!/usr/bin/env bash
# ============================================================================
# Cloud Provider: DigitalOcean
# ============================================================================
# Prerequisites:
#   brew install doctl
#   doctl auth init                     (paste API token)
#   doctl compute ssh-key import mesh-deploy --public-key-file ~/.ssh/id_ed25519.pub

PROVIDER_NAME="digitalocean"
PROVIDER_CLI="doctl"
PROVIDER_SSH_USER="root"
PROVIDER_SERVER_TYPE="${MESH_SERVER_TYPE:-s-1vcpu-2gb}"    # 1 vCPU, 2GB (~$6/mo)
PROVIDER_IMAGE="${MESH_IMAGE:-debian-12-x64}"
PROVIDER_SSH_KEY="${MESH_SSH_KEY:-}"                        # Set below in preflight

# Region name → DO region slug
# Full list: doctl compute region list
declare -A PROVIDER_LOCATIONS=(
    ["us-east"]="nyc1"          # New York
    ["us-west"]="sfo3"          # San Francisco
    ["eu-central"]="fra1"       # Frankfurt
    ["ap-southeast"]="sgp1"     # Singapore
)

provider_preflight() {
    if ! command -v doctl &>/dev/null; then
        echo "ERROR: doctl CLI not found"
        echo "  Install: brew install doctl"
        echo "  Then:    doctl auth init"
        return 1
    fi
    if ! doctl account get &>/dev/null; then
        echo "ERROR: doctl not authenticated"
        echo "  Run: doctl auth init"
        return 1
    fi
    # Find the SSH key fingerprint
    if [ -z "$PROVIDER_SSH_KEY" ]; then
        PROVIDER_SSH_KEY=$(doctl compute ssh-key list --format Name,FingerPrint --no-header | grep mesh-deploy | awk '{print $2}')
        if [ -z "$PROVIDER_SSH_KEY" ]; then
            echo "ERROR: SSH key 'mesh-deploy' not found in DigitalOcean"
            echo "  Add it: doctl compute ssh-key import mesh-deploy --public-key-file ~/.ssh/id_ed25519.pub"
            return 1
        fi
    fi
    return 0
}

provider_create_server() {
    local name="$1" region="$2"
    local location="${PROVIDER_LOCATIONS[$region]}"

    if [ -z "$location" ]; then
        echo "ERROR: Region '$region' not supported by DigitalOcean provider"
        echo "  Supported: ${!PROVIDER_LOCATIONS[*]}"
        return 1
    fi

    local output
    output=$(doctl compute droplet create "$name" \
        --region "$location" \
        --size "$PROVIDER_SERVER_TYPE" \
        --image "$PROVIDER_IMAGE" \
        --ssh-keys "$PROVIDER_SSH_KEY" \
        --tag-name "mesh-hub" \
        --format ID \
        --no-header \
        --wait \
        2>&1)

    local droplet_id
    droplet_id=$(echo "$output" | tr -d '[:space:]')

    if [ -z "$droplet_id" ]; then
        echo "ERROR: Failed to create droplet: $output"
        return 1
    fi

    # Get public IP
    doctl compute droplet get "$droplet_id" --format PublicIPv4 --no-header | tr -d '[:space:]'
}

provider_get_server_ip() {
    local name="$1"
    doctl compute droplet list --format Name,PublicIPv4 --no-header 2>/dev/null \
        | grep "^$name " | awk '{print $2}'
}

provider_server_exists() {
    local name="$1"
    doctl compute droplet list --format Name --no-header 2>/dev/null | grep -qx "$name"
}

provider_delete_server() {
    local name="$1"
    doctl compute droplet delete "$name" --force
}

provider_list_regions() {
    echo "${!PROVIDER_LOCATIONS[*]}"
}
