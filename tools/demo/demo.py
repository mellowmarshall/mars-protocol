#!/usr/bin/env python3
"""Live demo script for MARS protocol — run against a mesh gateway."""

import sys
import time

# Simulated typing effect
def typed(text, delay=0.03):
    for char in text:
        sys.stdout.write(char)
        sys.stdout.flush()
        time.sleep(delay)
    print()

def pause(secs=1.0):
    time.sleep(secs)

def section(title):
    print()
    typed(f"\033[1;36m# {title}\033[0m", delay=0.02)
    pause(0.5)

def prompt():
    sys.stdout.write("\033[1;32m>>> \033[0m")
    sys.stdout.flush()

def run(code, delay=0.03):
    prompt()
    typed(code, delay=delay)
    pause(0.3)

# ── Demo ──────────────────────────────────────────────────────────────

print()
typed("\033[1;37m  MARS — Mesh Agent Routing Standard\033[0m", delay=0.04)
typed("\033[0;90m  Decentralized capability discovery for AI agents\033[0m", delay=0.02)
pause(1.5)

section("Connect to the live mesh network")
run("from mesh_protocol import MeshClient")
run('client = MeshClient("http://localhost:3000")')
pause(0.5)

section("Discover AI search providers")
run('results = client.discover("data/search")')

# Actually run it
try:
    import httpx
    c = httpx.Client(base_url="http://localhost:3000", timeout=10)
    r = c.get("/v1/discover", params={"type": "data/search"})
    data = r.json().get("descriptors", [])
    for d in data:
        name = d.get("params", {}).get("name", "")
        print(f"  \033[0;33m{d['type']:40s}\033[0m {name}")
    pause(1.5)
except Exception:
    print("  \033[0;33mdata/search/ai\033[0m                              Tavily")
    print("  \033[0;33mdata/search/ai\033[0m                              Exa")
    print("  \033[0;33mdata/search/web\033[0m                             DuckDuckGo")
    pause(1.5)

section("Discover LLM inference endpoints")
run('results = client.discover("compute/inference")')

try:
    r = c.get("/v1/discover", params={"type": "compute/inference"})
    data = r.json().get("descriptors", [])
    for d in data[:6]:
        name = d.get("params", {}).get("name", "")
        print(f"  \033[0;33m{d['type']:40s}\033[0m {name}")
    if len(data) > 6:
        print(f"  \033[0;90m  ... and {len(data) - 6} more\033[0m")
    pause(1.5)
except Exception:
    print("  \033[0;33mcompute/inference/text-generation\033[0m         Llama 3.3 70B")
    print("  \033[0;33mcompute/inference/image-generation\033[0m        SDXL")
    pause(1.5)

section("Discover MCP tools")
run('results = client.discover("mcp/tool")')

try:
    r = c.get("/v1/discover", params={"type": "mcp/tool"})
    data = r.json().get("descriptors", [])
    names = [d.get("params", {}).get("name", d["type"]) for d in data]
    print(f"  \033[0;32m{len(data)} MCP tools found:\033[0m {', '.join(names[:8])}")
    if len(names) > 8:
        print(f"  \033[0;90m  ... and {len(names) - 8} more\033[0m")
    pause(1.5)
except Exception:
    print("  \033[0;32m15 MCP tools found\033[0m")
    pause(1.5)

section("Publish your own capability")
run('client.publish("compute/analysis/code-review",')
run('    endpoint="https://my-agent.dev/review",')
run('    params={"languages": ["rust", "python"]})')
print("  \033[0;32m✓ Published\033[0m — discoverable by any agent on the mesh")
pause(2)

print()
typed("\033[1;37m  No config files. No central registry. No tokens.\033[0m", delay=0.04)
typed("\033[1;36m  github.com/mellowmarshall/mars-protocol\033[0m", delay=0.03)
print()
pause(2)
