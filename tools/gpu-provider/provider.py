#!/usr/bin/env python3
"""
MARS GPU Provider Agent

Turns any machine with a GPU + Ollama into a mesh inference provider.
Interactive setup on first run. Auto-detects everything.

Usage:
    python provider.py                    # Interactive setup
    python provider.py --config mars.json # Use saved config (headless)

Prerequisites:
    pip install httpx
    # Ollama must be running: https://ollama.com
"""

from __future__ import annotations

import argparse
import json
import logging
import subprocess
import sys
import time
import webbrowser
from pathlib import Path
from typing import Any

import httpx

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger("mars-gpu-provider")

CONFIG_FILE = Path("mars-provider.json")

# GPU throughput benchmarks (tokens/sec for Llama 70B or equivalent)
# and typical rental/electricity cost per hour.
# Used to calculate realistic pricing suggestions.
GPU_BENCHMARKS: dict[str, dict[str, float]] = {
    # Consumer GPUs (electricity only: TDP * $/kWh, assume $0.15/kWh)
    "4090":  {"tok_per_sec": 25, "cost_per_hr": 0.08},   # 350W TDP
    "4080":  {"tok_per_sec": 18, "cost_per_hr": 0.05},   # 320W
    "3090":  {"tok_per_sec": 15, "cost_per_hr": 0.06},   # 350W
    "4070":  {"tok_per_sec": 12, "cost_per_hr": 0.03},   # 200W
    "3080":  {"tok_per_sec": 12, "cost_per_hr": 0.05},   # 320W
    "3060":  {"tok_per_sec": 6,  "cost_per_hr": 0.03},   # 170W
    # Data center GPUs (typical rental rates)
    "a100":  {"tok_per_sec": 40, "cost_per_hr": 0.80},
    "h100":  {"tok_per_sec": 80, "cost_per_hr": 2.00},
    "l40":   {"tok_per_sec": 30, "cost_per_hr": 0.60},
    "a10g":  {"tok_per_sec": 15, "cost_per_hr": 0.40},
}

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


def detect_region() -> str:
    """Attempt to auto-detect region from IP geolocation."""
    try:
        r = httpx.get("https://ipapi.co/json/", timeout=5)
        data = r.json()
        country = data.get("country_code", "")
        region = data.get("region", "")
        city = data.get("city", "")
        if country == "US":
            # Map US regions
            eastern = {"VA", "NY", "NJ", "PA", "MD", "DC", "NC", "SC", "GA", "FL", "MA", "CT"}
            state = data.get("region_code", "")
            if state in eastern:
                return "us-east"
            return "us-west"
        elif country in ("DE", "FR", "NL", "BE", "AT", "CH", "PL", "CZ"):
            return "eu-central"
        elif country in ("GB", "IE", "SE", "NO", "DK", "FI"):
            return "eu-west"
        elif country in ("SG", "MY", "TH", "VN", "PH", "ID"):
            return "ap-southeast"
        elif country in ("JP", "KR", "TW"):
            return "ap-northeast"
        elif country in ("AU", "NZ"):
            return "ap-south"
        return f"{city}, {region}" if city else country.lower()
    except Exception:
        return "unknown"


def suggest_price(gpu_name: str) -> dict[str, float] | None:
    """Calculate a realistic price range based on GPU throughput and costs.

    Returns cost_per_1k (break-even), suggested_min, suggested_max.
    Pricing is based on: (cost_per_hour / tokens_per_hour) * 1000 + margin.
    """
    name_lower = gpu_name.lower()
    bench = None
    for key, data in GPU_BENCHMARKS.items():
        if key in name_lower:
            bench = data
            break

    if not bench:
        return None

    tokens_per_hr = bench["tok_per_sec"] * 3600
    cost_per_mtok = (bench["cost_per_hr"] / tokens_per_hr) * 1_000_000

    return {
        "breakeven": round(cost_per_mtok, 2),
        "min": round(cost_per_mtok * 2, 2),      # 2x margin (floor)
        "max": round(cost_per_mtok * 5, 2),      # 5x margin (ceiling)
        "suggested": round(cost_per_mtok * 3, 2), # 3x margin (default)
        "tok_per_sec": bench["tok_per_sec"],
        "cost_per_hr": bench["cost_per_hr"],
    }


# ── Ngrok Tunnel ──────────────────────────────────────────────────────


def start_ngrok(port: int = 11434) -> str | None:
    """Start an ngrok tunnel and return the public URL."""
    try:
        subprocess.run(["ngrok", "version"], capture_output=True, timeout=5, check=True)
    except (FileNotFoundError, subprocess.TimeoutExpired, subprocess.CalledProcessError):
        return None

    _proc = subprocess.Popen(
        ["ngrok", "http", str(port), "--log=stdout", "--log-format=json"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )

    for _ in range(15):
        time.sleep(1)
        try:
            r = httpx.get("http://127.0.0.1:4040/api/tunnels", timeout=3)
            tunnels = r.json().get("tunnels", [])
            for t in tunnels:
                public_url = t.get("public_url", "")
                if public_url.startswith("https://"):
                    return public_url
        except Exception:
            continue
    return None


# ── Interactive Setup ─────────────────────────────────────────────────


def prompt_yn(question: str, default: bool = True) -> bool:
    """Prompt yes/no with a default."""
    hint = "[Y/n]" if default else "[y/N]"
    answer = input(f"  {question} {hint}: ").strip().lower()
    if not answer:
        return default
    return answer in ("y", "yes")


def prompt_str(question: str, default: str = "") -> str:
    """Prompt for a string with optional default."""
    if default:
        answer = input(f"  {question} [{default}]: ").strip()
        return answer if answer else default
    return input(f"  {question}: ").strip()


def prompt_float(question: str, default: float) -> float:
    """Prompt for a float with default."""
    answer = input(f"  {question} [{default}]: ").strip()
    if not answer:
        return default
    try:
        return float(answer)
    except ValueError:
        print(f"  Invalid number, using default: {default}")
        return default


def interactive_setup(
    gpu: dict[str, Any],
    models: list[dict[str, Any]],
    ollama_url: str,
) -> dict[str, Any]:
    """Walk the user through first-time setup. Returns a config dict."""
    print()
    print("  ╔══════════════════════════════════════════════════════╗")
    print("  ║         MARS GPU Provider — First Time Setup        ║")
    print("  ╚══════════════════════════════════════════════════════╝")
    print()
    print(f"  Detected: {gpu['gpu_name']} ({gpu['vram_mb']} MB VRAM)")
    print(f"  Ollama:   {len(models)} model(s) installed")
    for m in models:
        print(f"    - {m['name']} ({m['size_gb']}GB, {m['parameter_size'] or '?'} params)")
    print()

    # ── Monetization ──
    monetize = prompt_yn("Would you like to earn money for inference requests?")

    price = 0.0
    stripe_account = ""
    if monetize:
        print()
        pricing = suggest_price(gpu["gpu_name"])
        if pricing:
            print(f"  Estimated throughput: ~{pricing['tok_per_sec']:.0f} tokens/sec")
            print(f"  Your electricity cost: ~${pricing['cost_per_hr']:.2f}/hr")
            print(f"  Break-even price: ${pricing['breakeven']:.2f}/M tokens")
            print(f"  Suggested range:  ${pricing['min']:.2f} - ${pricing['max']:.2f}/M tokens")
            print()
            print(f"  For comparison (per million tokens):")
            print(f"    OpenAI GPT-4o:       $5.00/M tokens")
            print(f"    Together AI Llama:   $0.88/M tokens")
            print(f"    Groq Llama 70B:      $0.59/M tokens")
            print(f"    Your break-even:     ${pricing['breakeven']:.2f}/M tokens")
            default_price = pricing["suggested"]
        else:
            print("  Could not auto-detect pricing for your GPU.")
            print("  Typical range: $0.50-5.00/M tokens")
            default_price = 2.0
        price = prompt_float("Price per million tokens (USD)", default_price)
        print()

        # Stripe
        print("  To receive payments, you need a Stripe account.")
        print("  This takes about 2 minutes — just name, email, and bank info.")
        has_stripe = prompt_yn("Do you already have a Stripe Connect account?", default=False)

        if has_stripe:
            stripe_account = prompt_str("Stripe account ID (acct_xxx)")
        else:
            print()
            print("  We'll open Stripe Connect onboarding in your browser.")
            print("  After completing setup, paste your account ID here.")
            print()
            print("  → Go to: https://connect.stripe.com/express")
            print()
            try:
                webbrowser.open("https://connect.stripe.com/express")
            except Exception:
                pass
            stripe_account = prompt_str("Paste your Stripe account ID after onboarding (acct_xxx)")

        if price > 0 and not stripe_account:
            print()
            print("  ⚠ No Stripe account — running in free mode.")
            print("    Add --stripe-account later to start earning.")
            price = 0.0

    # ── Region ──
    print()
    print("  Detecting your region...")
    auto_region = detect_region()
    print(f"  Auto-detected: {auto_region}")
    use_auto = prompt_yn(f"Use '{auto_region}'?")
    region = auto_region if use_auto else prompt_str("Enter region (e.g. us-east, eu-central)")

    # ── Models ──
    print()
    if len(models) > 1:
        print("  Which models would you like to share?")
        selected = []
        for m in models:
            share = prompt_yn(f"{m['name']} ({m['size_gb']}GB)")
            if share:
                selected.append(m["name"])
        if not selected:
            print("  No models selected — sharing all.")
            selected = [m["name"] for m in models]
    else:
        selected = [m["name"] for m in models]

    # ── Gateway ──
    print()
    gateway = prompt_str("Mesh gateway URL", "http://localhost:3000")

    # ── Build config ──
    config = {
        "gateway": gateway,
        "ollama_url": ollama_url,
        "price_per_mtok": price,
        "stripe_account": stripe_account,
        "region": region,
        "models": selected,
        "refresh_secs": 1800,
    }

    # Save config
    CONFIG_FILE.write_text(json.dumps(config, indent=2))
    print()
    print(f"  ✓ Config saved to {CONFIG_FILE}")
    print(f"    Next time, run: python provider.py --config {CONFIG_FILE}")
    print()

    return config


# ── Descriptor Building ───────────────────────────────────────────────


def build_descriptors(
    gpu: dict[str, Any],
    models: list[dict[str, Any]],
    endpoint: str,
    config: dict[str, Any],
) -> list[dict[str, Any]]:
    """Build mesh descriptors from config."""
    descriptors = []
    price = config.get("price_per_mtok", 0.0)
    region = config.get("region", "unknown")
    stripe = config.get("stripe_account", "")

    for model in models:
        if "embed" in model["name"].lower():
            cap_type = "compute/inference/embeddings"
        elif "vision" in model["name"].lower() or "llava" in model["name"].lower():
            cap_type = "compute/inference/vision"
        elif "code" in model["name"].lower() or "starcoder" in model["name"].lower():
            cap_type = "compute/inference/code-generation"
        else:
            cap_type = "compute/inference/text-generation"

        params: dict[str, Any] = {
            "name": f"{model['name']} ({gpu['gpu_name']})",
            "provider": "mars-gpu-provider",
            "model": model["name"],
            "parameter_size": model.get("parameter_size", ""),
            "quantization": model.get("quantization", ""),
            "gpu": gpu["gpu_name"],
            "vram_mb": gpu["vram_mb"],
            "region": region,
            "ollama_api": f"{endpoint}/api/generate",
            "openai_compat": f"{endpoint}/v1/chat/completions",
        }

        if price > 0:
            params["price_per_mtok"] = price
            params["currency"] = "USD"
            params["accepts_payment"] = True
            if stripe:
                params["stripe_account"] = stripe
                params["payment_methods"] = ["stripe"]
        else:
            params["price_per_mtok"] = 0.0
            params["accepts_payment"] = False

        descriptors.append({"type": cap_type, "endpoint": endpoint, "params": params})

    return descriptors


def publish_descriptors(gateway_url: str, descriptors: list[dict[str, Any]]) -> list[str]:
    """Publish descriptors to the mesh."""
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
            time.sleep(7)
    client.close()
    return ids


# ── Main ──────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description="MARS GPU Provider Agent")
    parser.add_argument("--config", type=Path, default=None,
                        help="Path to saved config (skip interactive setup)")
    parser.add_argument("--ollama", default="http://localhost:11434",
                        help="Ollama API URL (default: http://localhost:11434)")
    parser.add_argument("--endpoint", default=None,
                        help="Override public endpoint (skip ngrok)")
    parser.add_argument("--no-ngrok", action="store_true",
                        help="Don't auto-start ngrok")
    args = parser.parse_args()

    # Detect hardware
    gpu = detect_gpu()
    models = detect_ollama_models(args.ollama)

    if not models:
        print()
        print("  No Ollama models found. Is Ollama running?")
        print()
        print("  Install Ollama:  curl -fsSL https://ollama.com/install.sh | sh")
        print("  Start Ollama:    ollama serve")
        print("  Pull a model:    ollama pull llama4")
        print()
        sys.exit(1)

    # Load or create config
    config_path = args.config or CONFIG_FILE
    if config_path.exists():
        config = json.loads(config_path.read_text())
        print(f"\n  Loaded config from {config_path}")
    else:
        config = interactive_setup(gpu, models, args.ollama)

    # Filter to selected models
    selected_names = set(config.get("models", [m["name"] for m in models]))
    selected_models = [m for m in models if m["name"] in selected_names]
    if not selected_models:
        selected_models = models

    # Determine endpoint
    if args.endpoint:
        endpoint = args.endpoint
    elif not args.no_ngrok:
        print("  Starting ngrok tunnel...")
        ollama_port = int(args.ollama.rsplit(":", 1)[-1])
        ngrok_url = start_ngrok(ollama_port)
        if ngrok_url:
            endpoint = ngrok_url
            print(f"  ✓ Public endpoint: {endpoint}")
        else:
            print("  ⚠ Ngrok not available — using localhost (only reachable locally)")
            print("    Install ngrok: https://ngrok.com/download")
            endpoint = args.ollama
    else:
        endpoint = args.ollama

    # Build and publish
    descriptors = build_descriptors(gpu, selected_models, endpoint, config)
    gateway = config.get("gateway", "http://localhost:3000")
    refresh = config.get("refresh_secs", 1800)
    price = config.get("price_per_mtok", 0.0)

    print()
    print("  ╔══════════════════════════════════════════════════════╗")
    print("  ║           MARS GPU Provider — Starting              ║")
    print("  ╚══════════════════════════════════════════════════════╝")
    print()
    print(f"  GPU:      {gpu['gpu_name']} ({gpu['vram_mb']} MB VRAM)")
    print(f"  Models:   {len(selected_models)}")
    print(f"  Endpoint: {endpoint}")
    print(f"  Price:    {'FREE' if price == 0 else f'${price:.2f}/M tokens'}")
    print(f"  Region:   {config.get('region', 'unknown')}")
    if config.get("stripe_account"):
        print(f"  Stripe:   {config['stripe_account']}")
    print(f"  Gateway:  {gateway}")
    print(f"  Refresh:  every {refresh}s")
    print()
    for desc in descriptors:
        p = desc["params"]
        print(f"  {desc['type']:45s} {p['model']}")
    print()

    while True:
        ids = publish_descriptors(gateway, descriptors)
        log.info("Published %d/%d. Next refresh in %ds.", len(ids), len(descriptors), refresh)
        try:
            time.sleep(refresh)
        except KeyboardInterrupt:
            print("\n  Shutting down. Your GPU is no longer on the mesh.")
            break


if __name__ == "__main__":
    main()
