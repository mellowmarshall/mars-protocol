#!/usr/bin/env bash
# ============================================================================
# MARS Network Re-seeder — runs as a cron job on the hub
# ============================================================================
# Starts an ephemeral gateway, runs the seeder, then stops the gateway.
# Designed to run every 30 minutes to keep descriptors alive (TTL=3600).

set -euo pipefail

LOG_TAG="mars-reseed"
GATEWAY_PORT=3001
HUB_ADDR="127.0.0.1:4433"
SEEDER_DIR="/opt/mars-seeder"

logger -t "$LOG_TAG" "starting reseed"

# Kill any stale gateway from a previous run
pkill -f "mesh-gateway.*$GATEWAY_PORT" 2>/dev/null || true
sleep 1

# Temporarily allow open stores
sed -i 's/store_mode = "tenant-only"/store_mode = "open"/' /etc/mesh-hub/mesh-hub.toml
systemctl restart mesh-hub
sleep 5

# Start ephemeral gateway on a non-standard port
/usr/local/bin/mesh-gateway --seed "$HUB_ADDR" --listen "127.0.0.1:$GATEWAY_PORT" &
GW_PID=$!
sleep 5

# Run seeder
cd "$SEEDER_DIR"
python3 seed.py --gateway "http://127.0.0.1:$GATEWAY_PORT" 2>&1 | logger -t "$LOG_TAG"

# Cleanup
kill "$GW_PID" 2>/dev/null || true
sleep 1

# Restore tenant-only
sed -i 's/store_mode = "open"/store_mode = "tenant-only"/' /etc/mesh-hub/mesh-hub.toml
systemctl restart mesh-hub

logger -t "$LOG_TAG" "reseed complete"
