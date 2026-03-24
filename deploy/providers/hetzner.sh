#!/usr/bin/env bash
# ============================================================================
# Cloud Provider: Hetzner Cloud
# ============================================================================
# Prerequisites:
#   brew install hcloud
#   hcloud context create mesh-protocol   (paste API token)
#   hcloud ssh-key create --name mesh-deploy --public-key-from-file ~/.ssh/id_ed25519.pub

PROVIDER_NAME="hetzner"
PROVIDER_CLI="hcloud"
PROVIDER_SSH_USER="root"
PROVIDER_SERVER_TYPE="${MESH_SERVER_TYPE:-cpx11}"      # 2 vCPU, 2GB, 40GB (~$4.50/mo)
PROVIDER_IMAGE="${MESH_IMAGE:-debian-12}"
PROVIDER_SSH_KEY="${MESH_SSH_KEY:-mesh-deploy}"

# Region name → provider location code
declare -A PROVIDER_LOCATIONS=(
    ["us-east"]="ash"
    ["us-west"]="hil"
    ["eu-central"]="nbg1"
    ["ap-southeast"]="sin"
)

provider_preflight() {
    if ! command -v hcloud &>/dev/null; then
        echo "ERROR: hcloud CLI not found"
        echo "  Install: brew install hcloud"
        echo "  Then:    hcloud context create mesh-protocol"
        return 1
    fi
    if ! hcloud ssh-key describe "$PROVIDER_SSH_KEY" &>/dev/null; then
        echo "ERROR: SSH key '$PROVIDER_SSH_KEY' not found in Hetzner Cloud"
        echo "  Add it: hcloud ssh-key create --name $PROVIDER_SSH_KEY --public-key-from-file ~/.ssh/id_ed25519.pub"
        return 1
    fi
    return 0
}

provider_create_server() {
    local name="$1" region="$2"
    local location="${PROVIDER_LOCATIONS[$region]}"

    if [ -z "$location" ]; then
        echo "ERROR: Region '$region' not supported by Hetzner provider"
        echo "  Supported: ${!PROVIDER_LOCATIONS[*]}"
        return 1
    fi

    hcloud server create \
        --name "$name" \
        --type "$PROVIDER_SERVER_TYPE" \
        --image "$PROVIDER_IMAGE" \
        --location "$location" \
        --ssh-key "$PROVIDER_SSH_KEY" \
        --label "role=mesh-hub" \
        --label "region=$region" \
        >/dev/null

    # Return the IP
    hcloud server ip "$name"
}

provider_get_server_ip() {
    local name="$1"
    hcloud server ip "$name" 2>/dev/null || echo ""
}

provider_server_exists() {
    local name="$1"
    hcloud server describe "$name" &>/dev/null
}

provider_delete_server() {
    local name="$1"
    hcloud server delete "$name"
}

provider_list_regions() {
    echo "${!PROVIDER_LOCATIONS[*]}"
}
