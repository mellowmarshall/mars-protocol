#!/usr/bin/env python3
"""
MARS OpenAPI Bridge

Takes an OpenAPI/Swagger spec URL or file and publishes every endpoint
as a mesh descriptor. This is the widest funnel — millions of OpenAPI
specs exist, and each endpoint becomes discoverable on the mesh.

Usage:
    # From a URL
    python openapi_bridge.py --gateway http://localhost:3000 \
        --spec https://petstore.swagger.io/v2/swagger.json

    # From a local file
    python openapi_bridge.py --gateway http://localhost:3000 \
        --spec ./openapi.yaml

    # With a type prefix (organizes under a mesh category)
    python openapi_bridge.py --gateway http://localhost:3000 \
        --spec https://api.stripe.com/openapi.json \
        --prefix "compute/payments"

Prerequisites:
    pip install httpx pyyaml
"""

from __future__ import annotations

import argparse
import logging
import sys
import time
from pathlib import Path
from typing import Any

import httpx

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger("mars-openapi-bridge")


def load_spec(source: str) -> dict[str, Any]:
    """Load an OpenAPI spec from a URL or local file."""
    if source.startswith("http://") or source.startswith("https://"):
        log.info("Fetching spec from %s", source)
        r = httpx.get(source, timeout=30, follow_redirects=True)
        r.raise_for_status()
        content = r.text
    else:
        log.info("Loading spec from %s", source)
        content = Path(source).read_text()

    # Try JSON first, then YAML
    import json
    try:
        return json.loads(content)
    except json.JSONDecodeError:
        pass

    try:
        import yaml
        return yaml.safe_load(content)
    except ImportError:
        log.error("YAML spec detected but pyyaml not installed: pip install pyyaml")
        sys.exit(1)


def extract_endpoints(spec: dict[str, Any]) -> list[dict[str, Any]]:
    """Extract API endpoints from an OpenAPI spec."""
    endpoints = []

    # Get base URL
    base_url = ""
    if "servers" in spec and spec["servers"]:
        base_url = spec["servers"][0].get("url", "")
    elif "host" in spec:
        scheme = (spec.get("schemes") or ["https"])[0]
        base_path = spec.get("basePath", "")
        base_url = f"{scheme}://{spec['host']}{base_path}"

    api_title = spec.get("info", {}).get("title", "Unknown API")
    api_version = spec.get("info", {}).get("version", "")

    paths = spec.get("paths", {})
    for path, methods in paths.items():
        if not isinstance(methods, dict):
            continue

        for method, operation in methods.items():
            if method.startswith("x-") or method == "parameters":
                continue
            if not isinstance(operation, dict):
                continue

            op_id = operation.get("operationId", f"{method}_{path}")
            summary = operation.get("summary", "")
            description = operation.get("description", "")
            tags = operation.get("tags", [])

            # Build parameter summary
            params = operation.get("parameters", [])
            param_names = [p.get("name", "") for p in params if isinstance(p, dict)]

            endpoints.append({
                "path": path,
                "method": method.upper(),
                "operation_id": op_id,
                "summary": summary or description[:200],
                "tags": tags,
                "parameters": param_names[:10],  # Cap for descriptor size
                "endpoint": f"{base_url}{path}",
                "api_title": api_title,
                "api_version": api_version,
            })

    return endpoints


def infer_type(path: str, method: str, tags: list[str], prefix: str) -> str:
    """Infer a mesh capability type from an endpoint's path and tags."""
    if prefix:
        # Clean up the path into a type-safe suffix
        suffix = path.strip("/").replace("/", "-").replace("{", "").replace("}", "")
        return f"{prefix}/{method.lower()}/{suffix}" if suffix else f"{prefix}/{method.lower()}"

    # Auto-infer from tags or path
    tag = tags[0].lower().replace(" ", "-") if tags else ""
    clean_path = path.strip("/").split("/")[0] if path.strip("/") else "root"

    category = tag or clean_path
    return f"api/{category}/{method.lower()}{path}".replace("{", "").replace("}", "")


def build_descriptors(
    endpoints: list[dict[str, Any]],
    prefix: str,
) -> list[dict[str, Any]]:
    """Convert OpenAPI endpoints to mesh descriptors."""
    descriptors = []

    for ep in endpoints:
        cap_type = infer_type(ep["path"], ep["method"], ep["tags"], prefix)

        descriptors.append({
            "type": cap_type,
            "endpoint": ep["endpoint"],
            "params": {
                "name": f"{ep['api_title']}: {ep['operation_id']}",
                "description": ep["summary"][:500] if ep["summary"] else "",
                "method": ep["method"],
                "api": ep["api_title"],
                "version": ep["api_version"],
                "parameters": ep["parameters"],
                "protocol": "openapi",
            },
        })

    return descriptors


def publish(
    gateway_url: str,
    descriptors: list[dict[str, Any]],
    dry_run: bool = False,
) -> tuple[int, int]:
    """Publish descriptors to the mesh."""
    if dry_run:
        for d in descriptors:
            name = d["params"]["name"]
            print(f"  [dry-run] {d['type']:50s} {name}")
        return len(descriptors), 0

    client = httpx.Client(base_url=gateway_url, timeout=30)
    ok, fail = 0, 0

    for i, d in enumerate(descriptors):
        name = d["params"]["name"]
        try:
            r = client.post("/v1/publish", json=d)
            r.raise_for_status()
            did = r.json().get("descriptor_id", "?")
            print(f"  [ok]   {d['type']:50s} {name}")
            ok += 1
        except Exception as e:
            print(f"  [FAIL] {d['type']:50s} {name} ({e})")
            fail += 1
        if i < len(descriptors) - 1:
            time.sleep(7)

    client.close()
    return ok, fail


def main() -> None:
    parser = argparse.ArgumentParser(description="MARS OpenAPI Bridge")
    parser.add_argument("--gateway", default="http://localhost:3000",
                        help="Mesh gateway URL")
    parser.add_argument("--spec", required=True,
                        help="OpenAPI spec URL or file path")
    parser.add_argument("--prefix", default="",
                        help="Mesh type prefix (e.g. 'compute/payments')")
    parser.add_argument("--dry-run", action="store_true",
                        help="Preview without publishing")
    parser.add_argument("--methods", default="GET,POST,PUT,PATCH,DELETE",
                        help="HTTP methods to include (default: all)")
    args = parser.parse_args()

    spec = load_spec(args.spec)
    endpoints = extract_endpoints(spec)

    allowed_methods = set(args.methods.upper().split(","))
    endpoints = [e for e in endpoints if e["method"] in allowed_methods]

    api_title = spec.get("info", {}).get("title", "Unknown")
    print(f"\nMARS OpenAPI Bridge")
    print(f"API:       {api_title}")
    print(f"Endpoints: {len(endpoints)}")
    if args.prefix:
        print(f"Prefix:    {args.prefix}")
    print()

    descriptors = build_descriptors(endpoints, args.prefix)

    ok, fail = publish(args.gateway, descriptors, dry_run=args.dry_run)

    print(f"\nPublished: {ok}, Failed: {fail}")


if __name__ == "__main__":
    main()
