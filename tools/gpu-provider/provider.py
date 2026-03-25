#!/usr/bin/env python3
"""
MARS GPU Provider Agent

Turns any machine with a GPU + Ollama into a mesh inference provider.
Auto-detects hardware, publishes capabilities, proxies requests.

Usage:
    # Basic — auto-detect everything
    python provider.py --gateway http://localhost:3000

    # Specify what to advertise
    python provider.py --gateway http://localhost:3000 --price 0.10 --models llama3.3,mistral

    # With a public endpoint (if you have port forwarding / ngrok)
    python provider.py --gateway http://localhost:3000 --endpoint https://my-gpu.ngrok.io

Prerequisites:
    pip install httpx
    # Ollama must be running: https://ollama.com
"""

from __future__ import annotations

import argparse
import logging
import subprocess
import sys
import time
from typing import Any

import httpx

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger("mars-gpu-provider")

# ── Hardware Detection ────────────────────────────────────────────────


def detect_gpu() -> dict[str, Any]:
    """Detect GPU hardware via nvidia-smi."""
    try:
        result = subprocess.run(
            ["nvidia-smi", "--query-gpu=name,memory.total,driver_version,gpu_uuid",
             "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=10,
        )
        if result.returncode == 0:
            parts = [p.strip() for p in result.stdout.strip().split(",")]
            if len(parts) >= 3:
                return {
                    "gpu_name": parts[0],
                    "vram_mb": int(float(parts[1])),
                    "driver": parts[2],
                    "gpu_id": parts[3] if len(parts) > 3 else None,
                }
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass

    # Try AMD ROCm
    try:
        result = subprocess.run(
            ["rocm-smi", "--showproductname", "--csv"],
            capture_output=True, text=True, timeout=10,
        )
        if result.returncode == 0:
            return {"gpu_name": result.stdout.strip(), "vram_mb": 0, "driver": "rocm"}
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass

    return {"gpu_name": "unknown", "vram_mb": 0, "driver": "none"}


def detect_ollama_models(ollama_url: str = "http://localhost:11434") -> list[dict[str, Any]]:
    """Query Ollama for installed models."""
    try:
        r = httpx.get(f"{ollama_url}/api/tags", timeout=5)
        r.raise_for_status()
        models = r.json().get("models", [])
        return [
            {
                "name": m["name"],
                "size_gb": round(m.get("size", 0) / 1e9, 1),
                "parameter_size": m.get("details", {}).get("parameter_size", ""),
                "quantization": m.get("details", {}).get("quantization_level", ""),
                "family": m.get("details", {}).get("family", ""),
            }
            for m in models
        ]
    except Exception:
        return []


# ── Ngrok Tunnel ──────────────────────────────────────────────────────


def start_ngrok(port: int = 11434) -> str | None:
    """Start an ngrok tunnel to the given port and return the public URL.

    Returns None if ngrok isn't installed or fails to start.
    """
    try:
        subprocess.run(["ngrok", "version"], capture_output=True, timeout=5, check=True)
    except (FileNotFoundError, subprocess.TimeoutExpired, subprocess.CalledProcessError):
        return None

    log.info("Starting ngrok tunnel to port %d...", port)

    # Start ngrok in background
    proc = subprocess.Popen(
        ["ngrok", "http", str(port), "--log=stdout", "--log-format=json"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )

    # Wait for tunnel to establish, then query the local API
    for attempt in range(15):
        time.sleep(1)
        try:
            r = httpx.get("http://127.0.0.1:4040/api/tunnels", timeout=3)
            tunnels = r.json().get("tunnels", [])
            for t in tunnels:
                public_url = t.get("public_url", "")
                if public_url.startswith("https://"):
                    log.info("Ngrok tunnel established: %s", public_url)
                    return public_url
        except Exception:
            continue

    log.warning("Ngrok started but no tunnel URL found")
    return None


# ── Descriptor Publishing ─────────────────────────────────────────────


def build_descriptors(
    gpu: dict[str, Any],
    models: list[dict[str, Any]],
    endpoint: str,
    price_per_1k_tokens: float,
    region: str,
) -> list[dict[str, Any]]:
    """Build mesh descriptors for each model on this GPU."""
    descriptors = []

    for model in models:
        # Determine capability type from model family
        if "embed" in model["name"].lower():
            cap_type = "compute/inference/embeddings"
        elif "vision" in model["name"].lower() or "llava" in model["name"].lower():
            cap_type = "compute/inference/vision"
        elif "code" in model["name"].lower() or "starcoder" in model["name"].lower():
            cap_type = "compute/inference/code-generation"
        else:
            cap_type = "compute/inference/text-generation"

        descriptors.append({
            "type": cap_type,
            "endpoint": endpoint,
            "params": {
                "name": f"{model['name']} ({gpu['gpu_name']})",
                "provider": "mars-gpu-provider",
                "model": model["name"],
                "parameter_size": model.get("parameter_size", ""),
                "quantization": model.get("quantization", ""),
                "gpu": gpu["gpu_name"],
                "vram_mb": gpu["vram_mb"],
                "price_per_1k_tokens": price_per_1k_tokens,
                "currency": "USD",
                "region": region,
                "ollama_api": f"{endpoint}/api/generate",
                "openai_compat": f"{endpoint}/v1/chat/completions",
            },
        })

    return descriptors


def publish_descriptors(
    gateway_url: str,
    descriptors: list[dict[str, Any]],
) -> list[str]:
    """Publish all descriptors to the mesh and return their IDs."""
    client = httpx.Client(base_url=gateway_url, timeout=30)
    ids = []

    for i, desc in enumerate(descriptors):
        try:
            r = client.post("/v1/publish", json=desc)
            r.raise_for_status()
            did = r.json().get("descriptor_id", "unknown")
            log.info("Published: %s → %s", desc["params"]["name"], did)
            ids.append(did)
        except Exception as e:
            log.error("Failed to publish %s: %s", desc["params"]["name"], e)
        if i < len(descriptors) - 1:
            time.sleep(7)  # Rate limit: 10/min per publisher

    client.close()
    return ids


# ── Main Loop ─────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description="MARS GPU Provider Agent")
    parser.add_argument("--gateway", required=True, help="Mesh gateway URL")
    parser.add_argument("--ollama", default="http://localhost:11434", help="Ollama API URL")
    parser.add_argument("--endpoint", default=None,
                        help="Public endpoint for this provider (default: auto-detect via ngrok)")
    parser.add_argument("--no-ngrok", action="store_true",
                        help="Don't auto-start ngrok tunnel (use --endpoint or localhost)")
    parser.add_argument("--price", type=float, default=0.0,
                        help="Price per 1K tokens in USD (0 = free)")
    parser.add_argument("--region", default="unknown", help="Provider region (e.g. us-east)")
    parser.add_argument("--models", default=None,
                        help="Comma-separated model names to advertise (default: all installed)")
    parser.add_argument("--refresh", type=int, default=1800,
                        help="Seconds between re-publish cycles (default: 1800)")
    args = parser.parse_args()

    # Determine public endpoint
    if args.endpoint:
        endpoint = args.endpoint
    elif not args.no_ngrok:
        # Auto-start ngrok tunnel for zero-config public access
        ollama_port = int(args.ollama.rsplit(":", 1)[-1])
        ngrok_url = start_ngrok(ollama_port)
        if ngrok_url:
            endpoint = ngrok_url
        else:
            log.warning("Ngrok not available — using localhost (only reachable locally)")
            log.warning("Install ngrok: https://ngrok.com/download or use --endpoint")
            endpoint = args.ollama
    else:
        endpoint = args.ollama

    # Detect hardware
    log.info("Detecting hardware...")
    gpu = detect_gpu()
    log.info("GPU: %s (%d MB VRAM)", gpu["gpu_name"], gpu["vram_mb"])

    # Detect models
    log.info("Querying Ollama at %s...", args.ollama)
    all_models = detect_ollama_models(args.ollama)

    if not all_models:
        log.error("No models found. Is Ollama running? (ollama serve)")
        sys.exit(1)

    # Filter models if specified
    if args.models:
        wanted = set(args.models.split(","))
        models = [m for m in all_models if m["name"] in wanted]
        if not models:
            log.error("None of the specified models are installed: %s", args.models)
            log.info("Available models: %s", ", ".join(m["name"] for m in all_models))
            sys.exit(1)
    else:
        models = all_models

    log.info("Models to advertise: %s", ", ".join(m["name"] for m in models))

    # Build descriptors
    descriptors = build_descriptors(gpu, models, endpoint, args.price, args.region)

    # Publish loop
    log.info("Publishing %d capabilities to %s...", len(descriptors), args.gateway)

    while True:
        ids = publish_descriptors(args.gateway, descriptors)
        log.info("Published %d/%d descriptors. Next refresh in %ds.",
                 len(ids), len(descriptors), args.refresh)

        print("\n" + "=" * 60)
        print("  MARS GPU Provider — Online")
        print("=" * 60)
        print(f"  GPU:      {gpu['gpu_name']} ({gpu['vram_mb']} MB)")
        print(f"  Models:   {len(models)}")
        print(f"  Endpoint: {endpoint}")
        print(f"  Price:    {'FREE' if args.price == 0 else f'${args.price}/1K tokens'}")
        print(f"  Region:   {args.region}")
        print(f"  Refresh:  every {args.refresh}s")
        print()
        for desc in descriptors:
            p = desc["params"]
            print(f"  {desc['type']:45s} {p['model']}")
        print()

        try:
            time.sleep(args.refresh)
        except KeyboardInterrupt:
            log.info("Shutting down")
            break


if __name__ == "__main__":
    main()
