#!/usr/bin/env python3
"""
MARS GPU Provider Agent

Turns any machine with a GPU + Ollama into a mesh inference provider.
Interactive setup on first run. Auto-detects everything.

Usage:
    python provider.py                    # Interactive setup
    python provider.py --config mars.json # Use saved config (headless)
    python provider.py --install          # Install as system service
    python provider.py --uninstall        # Remove system service
    python provider.py --status           # Check service status

Prerequisites:
    pip install httpx
    # Ollama must be running: https://ollama.com
"""

from __future__ import annotations

import argparse
import getpass
import json
import logging
import os
import signal
import subprocess
import sys
import textwrap
import time
import webbrowser
from pathlib import Path
from typing import Any

import httpx

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger("mars-gpu-provider")

CONFIG_FILE = Path("mars-provider.json")
PID_DIR = Path.home() / ".config" / "mars"
PID_FILE = PID_DIR / "provider.pid"
SERVICE_NAME = "mars-gpu-provider"
LAUNCHD_LABEL = "dev.mars-protocol.gpu-provider"

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


# ── PID File Management ──────────────────────────────────────────────


def write_pid_file() -> None:
    """Write the current process PID to the PID file."""
    PID_DIR.mkdir(parents=True, exist_ok=True)
    PID_FILE.write_text(str(os.getpid()))


def remove_pid_file() -> None:
    """Remove the PID file if it exists."""
    try:
        PID_FILE.unlink(missing_ok=True)
    except OSError:
        pass


def check_existing_instance() -> bool:
    """Check if another provider instance is already running.

    Returns True if a running instance was detected.
    """
    if not PID_FILE.exists():
        return False
    try:
        pid = int(PID_FILE.read_text().strip())
        # Send signal 0 to check if process exists (does not kill it)
        os.kill(pid, 0)
        return True
    except (ValueError, OSError):
        # PID file is stale — process is not running
        remove_pid_file()
        return False


# ── Service Installation ─────────────────────────────────────────────


def _resolve_config_path() -> Path:
    """Find the config file, resolving to an absolute path."""
    config_path = Path.cwd() / CONFIG_FILE
    if config_path.exists():
        return config_path.resolve()
    # Check next to the script itself
    script_dir = Path(__file__).resolve().parent
    alt = script_dir / CONFIG_FILE
    if alt.exists():
        return alt.resolve()
    return config_path.resolve()


def _generate_systemd_unit(provider_script: Path, config_path: Path) -> str:
    """Generate a systemd user service unit file."""
    python = Path(sys.executable).resolve()
    return textwrap.dedent(f"""\
        [Unit]
        Description=MARS GPU Provider Agent
        After=network-online.target
        Wants=network-online.target

        [Service]
        Type=simple
        ExecStart={python} {provider_script} --config {config_path} --service
        Restart=on-failure
        RestartSec=10
        Environment=PYTHONUNBUFFERED=1

        [Install]
        WantedBy=default.target
    """)


def _generate_launchd_plist(provider_script: Path, config_path: Path) -> str:
    """Generate a macOS LaunchAgent plist."""
    python = Path(sys.executable).resolve()
    log_dir = Path.home() / "Library" / "Logs" / "mars-gpu-provider"
    return textwrap.dedent(f"""\
        <?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
          "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
        <plist version="1.0">
        <dict>
            <key>Label</key>
            <string>{LAUNCHD_LABEL}</string>
            <key>ProgramArguments</key>
            <array>
                <string>{python}</string>
                <string>{provider_script}</string>
                <string>--config</string>
                <string>{config_path}</string>
                <string>--service</string>
            </array>
            <key>RunAtLoad</key>
            <true/>
            <key>KeepAlive</key>
            <true/>
            <key>StandardOutPath</key>
            <string>{log_dir}/stdout.log</string>
            <key>StandardErrorPath</key>
            <string>{log_dir}/stderr.log</string>
            <key>EnvironmentVariables</key>
            <dict>
                <key>PYTHONUNBUFFERED</key>
                <string>1</string>
            </dict>
        </dict>
        </plist>
    """)


def do_install() -> None:
    """Install the MARS GPU provider as a system service."""
    provider_script = Path(__file__).resolve()
    config_path = _resolve_config_path()

    # Step 1: ensure config exists
    if not config_path.exists():
        print(f"\n  Config file not found: {config_path}")
        print("  Running interactive setup first...\n")
        gpu = detect_gpu()
        models = detect_ollama_models()
        if not models:
            print("  No Ollama models found. Is Ollama running?")
            sys.exit(1)
        interactive_setup(gpu, models, "http://localhost:11434")
        if not config_path.exists():
            print("  Setup did not create a config file. Aborting.")
            sys.exit(1)

    platform = sys.platform

    if platform == "linux":
        _install_systemd(provider_script, config_path)
    elif platform == "darwin":
        _install_launchd(provider_script, config_path)
    elif platform == "win32":
        _install_windows_hint()
    else:
        print(f"  Unsupported platform: {platform}")
        sys.exit(1)


def _install_systemd(provider_script: Path, config_path: Path) -> None:
    """Install as a systemd user service (Linux)."""
    unit_dir = Path.home() / ".config" / "systemd" / "user"
    unit_dir.mkdir(parents=True, exist_ok=True)
    unit_file = unit_dir / f"{SERVICE_NAME}.service"

    unit_content = _generate_systemd_unit(provider_script, config_path)
    unit_file.write_text(unit_content)
    print(f"  Created service file: {unit_file}")

    # daemon-reload
    result = subprocess.run(
        ["systemctl", "--user", "daemon-reload"],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        print(f"  Warning: daemon-reload failed: {result.stderr.strip()}")

    # enable
    result = subprocess.run(
        ["systemctl", "--user", "enable", SERVICE_NAME],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        print(f"  Warning: enable failed: {result.stderr.strip()}")
    else:
        print(f"  Enabled {SERVICE_NAME}")

    # enable-linger so service survives logout
    user = getpass.getuser()
    result = subprocess.run(
        ["loginctl", "enable-linger", user],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        print(f"  Warning: enable-linger failed: {result.stderr.strip()}")
        print("  You may need to run: sudo loginctl enable-linger $USER")
    else:
        print(f"  Enabled linger for user '{user}'")

    # start
    result = subprocess.run(
        ["systemctl", "--user", "start", SERVICE_NAME],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        print(f"  Warning: start failed: {result.stderr.strip()}")
    else:
        print(f"  Started {SERVICE_NAME}")

    print()
    print("  ╔══════════════════════════════════════════════════════╗")
    print("  ║      MARS GPU Provider — Installed as Service       ║")
    print("  ╚══════════════════════════════════════════════════════╝")
    print()
    print(f"  Service:  {SERVICE_NAME}")
    print(f"  Config:   {config_path}")
    print(f"  Unit:     {unit_file}")
    print()
    print(f"  Check status:   python {provider_script.name} --status")
    print(f"  View logs:      journalctl --user -u {SERVICE_NAME} -f")
    print(f"  Uninstall:      python {provider_script.name} --uninstall")
    print()


def _install_launchd(provider_script: Path, config_path: Path) -> None:
    """Install as a macOS LaunchAgent."""
    plist_dir = Path.home() / "Library" / "LaunchAgents"
    plist_dir.mkdir(parents=True, exist_ok=True)
    plist_file = plist_dir / f"{LAUNCHD_LABEL}.plist"

    log_dir = Path.home() / "Library" / "Logs" / "mars-gpu-provider"
    log_dir.mkdir(parents=True, exist_ok=True)

    plist_content = _generate_launchd_plist(provider_script, config_path)
    plist_file.write_text(plist_content)
    print(f"  Created plist: {plist_file}")

    # load the agent
    result = subprocess.run(
        ["launchctl", "load", str(plist_file)],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        print(f"  Warning: launchctl load failed: {result.stderr.strip()}")
    else:
        print(f"  Loaded {LAUNCHD_LABEL}")

    print()
    print("  ╔══════════════════════════════════════════════════════╗")
    print("  ║      MARS GPU Provider — Installed as Service       ║")
    print("  ╚══════════════════════════════════════════════════════╝")
    print()
    print(f"  Service:  {LAUNCHD_LABEL}")
    print(f"  Config:   {config_path}")
    print(f"  Plist:    {plist_file}")
    print()
    print(f"  Check status:   python {provider_script.name} --status")
    print(f"  View logs:      cat ~/Library/Logs/mars-gpu-provider/stdout.log")
    print(f"  Uninstall:      python {provider_script.name} --uninstall")
    print()


def _install_windows_hint() -> None:
    """Print Windows installation instructions."""
    print()
    print("  ╔══════════════════════════════════════════════════════╗")
    print("  ║      MARS GPU Provider — Windows Instructions       ║")
    print("  ╚══════════════════════════════════════════════════════╝")
    print()
    print("  Automatic service installation is not supported on Windows.")
    print("  Options:")
    print()
    print("  1. Task Scheduler (recommended):")
    print("     - Open Task Scheduler (taskschd.msc)")
    print("     - Create Basic Task > 'MARS GPU Provider'")
    print("     - Trigger: 'When I log on'")
    print(f"     - Action: Start a program")
    print(f"       Program: {sys.executable}")
    print(f"       Arguments: {Path(__file__).resolve()} --config mars-provider.json --service")
    print()
    print("  2. WSL (if running in Windows Subsystem for Linux):")
    print("     - Use --install from within WSL (systemd support required)")
    print()


def do_uninstall() -> None:
    """Uninstall the MARS GPU provider service."""
    platform = sys.platform

    if platform == "linux":
        _uninstall_systemd()
    elif platform == "darwin":
        _uninstall_launchd()
    elif platform == "win32":
        print("  Remove the MARS GPU Provider task from Task Scheduler manually.")
    else:
        print(f"  Unsupported platform: {platform}")
        sys.exit(1)


def _uninstall_systemd() -> None:
    """Uninstall the systemd user service."""
    unit_file = Path.home() / ".config" / "systemd" / "user" / f"{SERVICE_NAME}.service"

    # stop
    subprocess.run(
        ["systemctl", "--user", "stop", SERVICE_NAME],
        capture_output=True, text=True,
    )
    print(f"  Stopped {SERVICE_NAME}")

    # disable
    subprocess.run(
        ["systemctl", "--user", "disable", SERVICE_NAME],
        capture_output=True, text=True,
    )
    print(f"  Disabled {SERVICE_NAME}")

    # remove unit file
    if unit_file.exists():
        unit_file.unlink()
        print(f"  Removed {unit_file}")
    else:
        print(f"  Unit file not found: {unit_file}")

    # daemon-reload
    subprocess.run(
        ["systemctl", "--user", "daemon-reload"],
        capture_output=True, text=True,
    )
    print(f"  Reloaded systemd daemon")

    # clean up PID file
    remove_pid_file()

    print()
    print(f"  MARS GPU Provider service has been uninstalled.")
    print()


def _uninstall_launchd() -> None:
    """Uninstall the macOS LaunchAgent."""
    plist_file = Path.home() / "Library" / "LaunchAgents" / f"{LAUNCHD_LABEL}.plist"

    # unload
    if plist_file.exists():
        subprocess.run(
            ["launchctl", "unload", str(plist_file)],
            capture_output=True, text=True,
        )
        print(f"  Unloaded {LAUNCHD_LABEL}")

        plist_file.unlink()
        print(f"  Removed {plist_file}")
    else:
        print(f"  Plist not found: {plist_file}")

    # clean up PID file
    remove_pid_file()

    print()
    print(f"  MARS GPU Provider service has been uninstalled.")
    print()


def do_status() -> None:
    """Check the status of the MARS GPU provider service."""
    platform = sys.platform

    if platform == "linux":
        result = subprocess.run(
            ["systemctl", "--user", "status", SERVICE_NAME],
            capture_output=True, text=True,
        )
        print(result.stdout)
        if result.stderr:
            print(result.stderr)
    elif platform == "darwin":
        result = subprocess.run(
            ["launchctl", "list", LAUNCHD_LABEL],
            capture_output=True, text=True,
        )
        if result.returncode == 0:
            print(result.stdout)
        else:
            print(f"  Service '{LAUNCHD_LABEL}' is not loaded.")
            if result.stderr:
                print(result.stderr)
    elif platform == "win32":
        print("  Check Task Scheduler for the MARS GPU Provider task.")
    else:
        print(f"  Unsupported platform: {platform}")


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
    parser.add_argument("--install", action="store_true",
                        help="Install as a system service (systemd/launchd)")
    parser.add_argument("--uninstall", action="store_true",
                        help="Uninstall the system service")
    parser.add_argument("--status", action="store_true",
                        help="Check the status of the system service")
    parser.add_argument("--service", action="store_true",
                        help="Run in service mode (no interactive prompts, log to stdout)")
    args = parser.parse_args()

    # Handle service management commands first
    if args.install:
        do_install()
        return

    if args.uninstall:
        do_uninstall()
        return

    if args.status:
        do_status()
        return

    # Check for existing running instance via PID file
    if check_existing_instance():
        pid = int(PID_FILE.read_text().strip())
        log.warning(
            "Another MARS GPU provider instance appears to be running (PID %d). "
            "If this is stale, remove %s and retry.",
            pid, PID_FILE,
        )
        if not args.service:
            answer = input("  Continue anyway? [y/N]: ").strip().lower()
            if answer not in ("y", "yes"):
                sys.exit(1)

    # Write PID file and ensure cleanup on exit
    write_pid_file()

    def _cleanup_pid(signum: int | None = None, frame: Any = None) -> None:
        remove_pid_file()
        if signum is not None:
            sys.exit(0)

    signal.signal(signal.SIGTERM, _cleanup_pid)

    # In service mode, suppress interactive prompts
    if args.service:
        # Reconfigure logging for journald (stdout)
        logging.basicConfig(
            level=logging.INFO,
            format="%(asctime)s %(levelname)s %(message)s",
            force=True,
        )

    # Detect hardware
    gpu = detect_gpu()
    models = detect_ollama_models(args.ollama)

    if not models:
        if args.service:
            log.error("No Ollama models found. Is Ollama running?")
        else:
            print()
            print("  No Ollama models found. Is Ollama running?")
            print()
            print("  Install Ollama:  curl -fsSL https://ollama.com/install.sh | sh")
            print("  Start Ollama:    ollama serve")
            print("  Pull a model:    ollama pull llama4")
            print()
        remove_pid_file()
        sys.exit(1)

    # Load or create config
    config_path = args.config or CONFIG_FILE
    if config_path.exists():
        config = json.loads(config_path.read_text())
        if args.service:
            log.info("Loaded config from %s", config_path)
        else:
            print(f"\n  Loaded config from {config_path}")
    elif args.service:
        log.error("Config file not found: %s. Run setup first.", config_path)
        remove_pid_file()
        sys.exit(1)
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
    elif not args.no_ngrok and not args.service:
        print("  Starting ngrok tunnel...")
        ollama_port = int(args.ollama.rsplit(":", 1)[-1])
        ngrok_url = start_ngrok(ollama_port)
        if ngrok_url:
            endpoint = ngrok_url
            print(f"  Public endpoint: {endpoint}")
        else:
            print("  Ngrok not available -- using localhost (only reachable locally)")
            print("    Install ngrok: https://ngrok.com/download")
            endpoint = args.ollama
    else:
        endpoint = args.ollama

    # Build and publish
    descriptors = build_descriptors(gpu, selected_models, endpoint, config)
    gateway = config.get("gateway", "http://localhost:3000")
    refresh = config.get("refresh_secs", 1800)
    price = config.get("price_per_mtok", 0.0)

    if args.service:
        log.info(
            "Starting: GPU=%s, models=%d, endpoint=%s, price=%s, gateway=%s",
            gpu["gpu_name"], len(selected_models), endpoint,
            "FREE" if price == 0 else f"${price:.2f}/M tokens", gateway,
        )
    else:
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
            if args.service:
                log.info("Shutting down. Your GPU is no longer on the mesh.")
            else:
                print("\n  Shutting down. Your GPU is no longer on the mesh.")
            remove_pid_file()
            break


if __name__ == "__main__":
    main()
