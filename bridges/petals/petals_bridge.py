#!/usr/bin/env python3
"""
MARS ↔ Petals Bridge

Connects to the Petals distributed inference swarm and publishes
available models as MARS mesh descriptors. Agents on the mesh can
discover and use models running across hundreds of volunteer GPUs.

Usage:
    # Connect to the public Petals swarm and publish to MARS
    python petals_bridge.py --gateway http://localhost:3000

    # Connect to a private swarm
    python petals_bridge.py --gateway http://localhost:3000 \
        --initial-peers /ip4/1.2.3.4/tcp/31337/p2p/QmPeer...

    # Use the health API instead of direct DHT connection
    python petals_bridge.py --gateway http://localhost:3000 --health-api

Prerequisites:
    pip install httpx

    For direct DHT connection (optional, richer data):
    pip install petals
"""

from __future__ import annotations

import argparse
import logging
import time
from typing import Any

import httpx

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger("mars-petals-bridge")

# Known public Petals swarm initial peers (may change — check petals.dev)
PUBLIC_INITIAL_PEERS = [
    "/ip4/159.89.214.152/tcp/31337/p2p/QmNwJZYbGmTjJmECCPSwUMM6NKQWvfT4u1BSL5upqoSGHQ",
]

# Well-known Petals chat endpoint (OpenAI-compatible)
PETALS_CHAT_ENDPOINT = "https://chat.petals.dev/api/v1/chat/completions"


# ── Discovery via Health API ──────────────────────────────────────────


def discover_via_health_api(
    health_url: str = "https://health.petals.dev/api/v1/state",
) -> list[dict[str, Any]]:
    """Query the Petals health monitor for available models and servers."""
    log.info("Querying Petals health API at %s...", health_url)

    try:
        r = httpx.get(health_url, timeout=30, follow_redirects=True)
        r.raise_for_status()
        data = r.json()
    except Exception as e:
        log.warning("Health API unavailable: %s", e)
        return []

    models = []
    model_info = data.get("model_info", {})

    for model_name, info in model_info.items():
        if not isinstance(info, dict):
            continue

        # Count active servers
        server_rows = info.get("server_rows", [])
        active_servers = sum(1 for s in server_rows if s.get("state") == "online")

        if active_servers == 0:
            continue

        # Calculate total throughput
        total_throughput = sum(
            s.get("throughput", 0) for s in server_rows if s.get("state") == "online"
        )

        models.append({
            "name": model_name,
            "active_servers": active_servers,
            "total_throughput": round(total_throughput, 1),
            "num_blocks": info.get("num_blocks", 0),
            "repository": info.get("repository", model_name),
        })

    log.info("Found %d active models on Petals swarm", len(models))
    return models


# ── Discovery via Hivemind DHT ────────────────────────────────────────


def discover_via_dht(
    initial_peers: list[str] | None = None,
) -> list[dict[str, Any]]:
    """Connect directly to the Petals hivemind DHT and list available models."""
    try:
        import hivemind
    except ImportError:
        log.error("hivemind not installed. Install with: pip install petals")
        log.error("Falling back to health API mode (--health-api)")
        return []

    peers = initial_peers or PUBLIC_INITIAL_PEERS
    log.info("Connecting to Petals DHT with %d initial peers...", len(peers))

    try:
        dht = hivemind.DHT(initial_peers=peers, client_mode=True, start=True)
        log.info("Connected to Petals DHT, peer ID: %s", dht.peer_id)
    except Exception as e:
        log.error("Failed to connect to Petals DHT: %s", e)
        return []

    # Query for known model prefixes
    known_models = [
        "meta-llama/Meta-Llama-3.1-405B-Instruct",
        "meta-llama/Meta-Llama-3.1-70B-Instruct",
        "meta-llama/Llama-3.3-70B-Instruct",
        "mistralai/Mixtral-8x22B-Instruct-v0.1",
        "bigscience/bloom",
        "bigscience/bloomz",
        "tiiuae/falcon-180B-chat",
        "stabilityai/StableBeluga2",
    ]

    models = []
    for model_name in known_models:
        try:
            # Check if model has active servers in the DHT
            # Check if model has active servers by querying block 0 in the DHT
            key = f"{model_name}.0"
            result = dht.get(key, latest=True)
            if result and result.value:
                models.append({
                    "name": model_name,
                    "active_servers": -1,  # Unknown from DHT alone
                    "total_throughput": 0,
                    "num_blocks": 0,
                    "repository": model_name,
                })
                log.info("  Found: %s", model_name)
        except Exception:
            continue

    dht.shutdown()
    return models


# ── Publish to MARS ───────────────────────────────────────────────────


def publish_models(
    gateway_url: str,
    models: list[dict[str, Any]],
    dry_run: bool = False,
) -> tuple[int, int]:
    """Publish Petals models as MARS mesh descriptors."""
    if not models:
        log.warning("No models to publish")
        return 0, 0

    client = httpx.Client(base_url=gateway_url, timeout=30) if not dry_run else None
    ok, fail = 0, 0

    for i, model in enumerate(models):
        short_name = model["name"].split("/")[-1]

        # Determine capability type
        name_lower = model["name"].lower()
        if "embed" in name_lower:
            cap_type = "compute/inference/embeddings"
        elif "vision" in name_lower or "llava" in name_lower:
            cap_type = "compute/inference/vision"
        elif "code" in name_lower or "starcoder" in name_lower:
            cap_type = "compute/inference/code-generation"
        else:
            cap_type = "compute/inference/text-generation"

        descriptor = {
            "type": cap_type,
            "endpoint": PETALS_CHAT_ENDPOINT,
            "params": {
                "name": f"{short_name} (Petals Swarm)",
                "description": f"Distributed inference via Petals — {model['active_servers']} active servers",
                "provider": "petals",
                "model": model["name"],
                "huggingface": f"https://huggingface.co/{model['repository']}",
                "active_servers": model["active_servers"],
                "total_throughput_tokens_per_sec": model["total_throughput"],
                "distributed": True,
                "auth": "none",
                "openai_compatible": True,
                "docs": "https://petals.dev",
            },
        }

        if dry_run:
            print(f"  [dry-run] {cap_type:45s} {short_name} ({model['active_servers']} servers)")
            ok += 1
            continue

        try:
            r = client.post("/v1/publish", json=descriptor)
            r.raise_for_status()
            did = r.json().get("descriptor_id", "?")
            print(f"  [ok]   {cap_type:45s} {short_name} ({model['active_servers']} servers) → {did}")
            ok += 1
        except Exception as e:
            print(f"  [FAIL] {cap_type:45s} {short_name} ({e})")
            fail += 1

        if not dry_run and i < len(models) - 1:
            time.sleep(7)

    if client:
        client.close()
    return ok, fail


# ── Main ──────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description="MARS ↔ Petals Bridge")
    parser.add_argument("--gateway", default="http://localhost:3000",
                        help="MARS mesh gateway URL")
    parser.add_argument("--health-api", action="store_true",
                        help="Use Petals health API instead of direct DHT connection")
    parser.add_argument("--health-url", default="https://health.petals.dev/api/v1/state",
                        help="Petals health API URL")
    parser.add_argument("--initial-peers", nargs="*", default=None,
                        help="Hivemind DHT initial peers (default: public swarm)")
    parser.add_argument("--refresh", type=int, default=300,
                        help="Seconds between refresh cycles (default: 300)")
    parser.add_argument("--dry-run", action="store_true",
                        help="Preview without publishing")
    parser.add_argument("--once", action="store_true",
                        help="Run once and exit (no refresh loop)")
    args = parser.parse_args()

    print("\nMARS ↔ Petals Bridge")
    print(f"Gateway: {args.gateway}")
    print(f"Mode:    {'health API' if args.health_api else 'direct DHT'}")
    print()

    while True:
        # Discover available models
        if args.health_api:
            models = discover_via_health_api(args.health_url)
        else:
            models = discover_via_dht(args.initial_peers)
            if not models:
                log.info("DHT discovery returned nothing, trying health API fallback...")
                models = discover_via_health_api(args.health_url)

        # Publish to MARS
        ok, fail = publish_models(args.gateway, models, dry_run=args.dry_run)

        print(f"\nPublished: {ok}, Failed: {fail}")

        if args.once or args.dry_run:
            break

        log.info("Next refresh in %ds...", args.refresh)
        try:
            time.sleep(args.refresh)
        except KeyboardInterrupt:
            log.info("Shutting down")
            break


if __name__ == "__main__":
    main()
