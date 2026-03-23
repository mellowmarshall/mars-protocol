#!/usr/bin/env bash
# ============================================================================
# AI Agent — Discover Capabilities on the Mesh
# ============================================================================
# This example shows an agent discovering what capabilities are available
# on the mesh — finding other agents that can generate text, analyze code,
# search the web, or perform any other registered capability.
#
# Prerequisites:
#   cargo build --release
#   export PATH="$PWD/target/release:$PATH"

set -euo pipefail

SEED_ADDR="your-hub.example.com:4433"     # Replace with your hub
IDENTITY="data/agent-identity.key"

# ── Discover: all inference capabilities ─────────────────────────────────────
# Hierarchical routing keys mean searching "compute/inference" returns ALL
# inference providers (text-generation, image, speech, embeddings, etc.)

echo "=== All inference capabilities ==="
mesh-node discover \
    --type "compute/inference" \
    --seed "$SEED_ADDR" \
    --identity "$IDENTITY"

# ── Discover: specific capability ────────────────────────────────────────────

echo ""
echo "=== Text generation specifically ==="
mesh-node discover \
    --type "compute/inference/text-generation" \
    --seed "$SEED_ADDR" \
    --identity "$IDENTITY"

# ── Discover: data sources ───────────────────────────────────────────────────

echo ""
echo "=== Data sources ==="
mesh-node discover \
    --type "data/search" \
    --seed "$SEED_ADDR" \
    --identity "$IDENTITY"

# ── Discover: everything under compute ───────────────────────────────────────

echo ""
echo "=== All compute capabilities ==="
mesh-node discover \
    --type "compute" \
    --seed "$SEED_ADDR" \
    --identity "$IDENTITY"
