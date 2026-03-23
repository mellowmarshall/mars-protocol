#!/usr/bin/env bash
# ============================================================================
# AI Agent — Publish Capabilities to the Mesh
# ============================================================================
# This example shows an AI agent (e.g., an OpenClaw agent) publishing its
# capabilities to the mesh so other agents can discover and invoke them.
#
# Prerequisites:
#   cargo build --release
#   export PATH="$PWD/target/release:$PATH"
#   A running hub or seed node at SEED_ADDR

set -euo pipefail

SEED_ADDR="your-hub.example.com:4433"     # Replace with your hub
IDENTITY="data/agent-identity.key"

# Generate agent identity (first run only)
if [ ! -f "$IDENTITY" ]; then
    mkdir -p "$(dirname "$IDENTITY")"
    mesh-node identity --generate --path "$IDENTITY"
    echo "Generated agent identity at $IDENTITY"
    echo ""
fi

# ── Publish: text generation capability ──────────────────────────────────────
# Other agents searching for "compute/inference/text-generation" will find this.

echo "Publishing text-generation capability..."
mesh-node publish \
    --type "compute/inference/text-generation" \
    --endpoint "https://my-agent.example.com/v1/generate" \
    --params '{"model":"llama-3.3-70b","max_tokens":4096,"formats":["text","json"]}' \
    --seed "$SEED_ADDR" \
    --identity "$IDENTITY"

# ── Publish: code review capability ──────────────────────────────────────────

echo ""
echo "Publishing code-review capability..."
mesh-node publish \
    --type "compute/analysis/code-review" \
    --endpoint "https://my-agent.example.com/v1/review" \
    --params '{"languages":["rust","python","typescript"],"max_file_size_kb":512}' \
    --seed "$SEED_ADDR" \
    --identity "$IDENTITY"

# ── Publish: web search capability ───────────────────────────────────────────

echo ""
echo "Publishing web-search capability..."
mesh-node publish \
    --type "data/search/web" \
    --endpoint "https://my-agent.example.com/v1/search" \
    --params '{"engines":["google","bing"],"max_results":50}' \
    --seed "$SEED_ADDR" \
    --identity "$IDENTITY"

echo ""
echo "Done. Other agents can now discover these capabilities on the mesh."
