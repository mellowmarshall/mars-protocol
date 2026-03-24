#!/usr/bin/env bash
# ============================================================================
# Cloud Provider: Vultr
# ============================================================================
# Prerequisites:
#   brew install vultr-cli
#   export VULTR_API_KEY="your-api-key"
#   vultr-cli ssh-key create --name mesh-deploy --key "$(cat ~/.ssh/id_ed25519.pub)"

PROVIDER_NAME="vultr"
PROVIDER_CLI="vultr-cli"
PROVIDER_SSH_USER="root"
PROVIDER_SERVER_TYPE="${MESH_SERVER_TYPE:-vc2-2c-4gb}"   # 2 vCPU, 4GB (~$24/mo)
PROVIDER_IMAGE="${MESH_IMAGE:-2136}"                      # Debian 12
PROVIDER_SSH_KEY="${MESH_SSH_KEY:-}"                       # Set below in preflight

# Region name → Vultr region ID
# Full list: vultr-cli regions list
declare -A PROVIDER_LOCATIONS=(
    ["us-east"]="ewr"           # New Jersey
    ["us-west"]="lax"           # Los Angeles
    ["eu-central"]="fra"        # Frankfurt
    ["ap-southeast"]="sgp"      # Singapore
)

provider_preflight() {
    if ! command -v vultr-cli &>/dev/null; then
        echo "ERROR: vultr-cli not found"
        echo "  Install: brew install vultr-cli"
        echo "  Then:    export VULTR_API_KEY='your-api-key'"
        return 1
    fi
    if [ -z "${VULTR_API_KEY:-}" ]; then
        echo "ERROR: VULTR_API_KEY not set"
        echo "  Get one at: https://my.vultr.com/settings/#settingsapi"
        echo "  Then: export VULTR_API_KEY='your-key'"
        return 1
    fi
    # Find the SSH key ID
    if [ -z "$PROVIDER_SSH_KEY" ]; then
        PROVIDER_SSH_KEY=$(vultr-cli ssh-key list | grep mesh-deploy | awk '{print $1}')
        if [ -z "$PROVIDER_SSH_KEY" ]; then
            echo "ERROR: SSH key 'mesh-deploy' not found in Vultr"
            echo "  Add it: vultr-cli ssh-key create --name mesh-deploy --key \"\$(cat ~/.ssh/id_ed25519.pub)\""
            return 1
        fi
    fi
    return 0
}

provider_create_server() {
    local name="$1" region="$2"
    local location="${PROVIDER_LOCATIONS[$region]}"

    if [ -z "$location" ]; then
        echo "ERROR: Region '$region' not supported by Vultr provider"
        echo "  Supported: ${!PROVIDER_LOCATIONS[*]}"
        return 1
    fi

    local output
    output=$(vultr-cli instance create \
        --label "$name" \
        --region "$location" \
        --plan "$PROVIDER_SERVER_TYPE" \
        --os "$PROVIDER_IMAGE" \
        --ssh-keys "$PROVIDER_SSH_KEY" \
        2>&1)

    local instance_id
    instance_id=$(echo "$output" | grep -oP 'ID\s+\K[a-f0-9-]+' | head -1)

    if [ -z "$instance_id" ]; then
        echo "ERROR: Failed to create server: $output"
        return 1
    fi

    # Wait for IP assignment
    local ip=""
    for _ in $(seq 1 30); do
        ip=$(vultr-cli instance get "$instance_id" | grep -oP 'MAIN IP\s+\K[0-9.]+' | head -1)
        if [ -n "$ip" ] && [ "$ip" != "0.0.0.0" ]; then
            break
        fi
        sleep 2
    done

    echo "$ip"
}

provider_get_server_ip() {
    local name="$1"
    local instance_id
    instance_id=$(vultr-cli instance list | grep "$name" | awk '{print $1}' | head -1)
    if [ -n "$instance_id" ]; then
        vultr-cli instance get "$instance_id" | grep -oP 'MAIN IP\s+\K[0-9.]+' | head -1
    fi
}

provider_server_exists() {
    local name="$1"
    vultr-cli instance list 2>/dev/null | grep -q "$name"
}

provider_delete_server() {
    local name="$1"
    local instance_id
    instance_id=$(vultr-cli instance list | grep "$name" | awk '{print $1}' | head -1)
    if [ -n "$instance_id" ]; then
        vultr-cli instance delete "$instance_id"
    fi
}

provider_list_regions() {
    echo "${!PROVIDER_LOCATIONS[*]}"
}
