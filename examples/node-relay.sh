#!/usr/bin/env bash
# ============================================================================
# Mesh Relay Node — Example Startup Script
# ============================================================================
# A relay node participates in the DHT, forwarding queries and caching
# descriptors. Run relay nodes to strengthen the mesh and reduce latency
# in your region.
#
# Prerequisites:
#   cargo build --release
#   export PATH="$PWD/target/release:$PATH"

set -euo pipefail

# ── Configuration ────────────────────────────────────────────────────────────

LISTEN="0.0.0.0:4433"
SEED_HUB="your-hub.example.com:4433"      # Replace with your hub's address
IDENTITY="data/relay-identity.key"

# ── Generate identity (first run only) ───────────────────────────────────────

if [ ! -f "$IDENTITY" ]; then
    mkdir -p "$(dirname "$IDENTITY")"
    mesh-node identity --generate --path "$IDENTITY"
    echo "Generated new relay identity at $IDENTITY"
fi

# ── Start relay ──────────────────────────────────────────────────────────────

echo "Starting mesh relay node..."
echo "  Listen:   $LISTEN"
echo "  Seed:     $SEED_HUB"
echo "  Identity: $IDENTITY"
echo ""

exec mesh-node start \
    --listen "$LISTEN" \
    --seed "$SEED_HUB" \
    --identity "$IDENTITY"
