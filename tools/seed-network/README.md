# MARS Network Seeder

Seeds the MARS mesh network with real-world service descriptors via the HTTP gateway.

Includes three source catalogs:
- **Public APIs** -- free/open REST APIs (weather, search, geocoding, GitHub, etc.)
- **MCP Skills** -- top Model Context Protocol tools from ClawHub
- **LLM Endpoints** -- HuggingFace Inference API models (text, image, speech, code)

## Prerequisites

- A running `mesh-gateway` instance (default: `http://localhost:3000`)
- Python 3.10+
- `httpx` (`pip install httpx`)

## Usage

```bash
# Preview what would be published (no network calls to the gateway)
./seed.py --dry-run

# Seed against local gateway
./seed.py

# Seed against a remote gateway
./seed.py --gateway https://mars-gw.example.com:3000
```

## Adding services

Each source file in `sources/` exports a plain list of dicts with this shape:

```python
{
    "type": "data/weather/current",           # hierarchical capability type
    "endpoint": "https://api.open-meteo.com/v1/forecast",  # service URL
    "params": {                                # optional metadata
        "name": "Open-Meteo",
        "auth": "none",
    },
}
```

Add entries to the appropriate source file or create a new module and import it
in `seed.py`.
