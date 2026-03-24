#!/usr/bin/env python3
"""Seed the MARS mesh network with real-world services."""

from __future__ import annotations

import argparse
import sys
import time
from typing import Any

import httpx

from sources.public_apis import PUBLIC_APIS
from sources.mcp_skills import MCP_SKILLS
from sources.llm_endpoints import LLM_ENDPOINTS


def publish_descriptor(
    client: httpx.Client | None,
    service: dict[str, Any],
    *,
    dry_run: bool = False,
) -> str | None:
    """Publish a single service descriptor to the mesh gateway.

    Returns the descriptor ID on success, or None on failure.
    """
    name = service.get("params", {}).get("name", service["endpoint"])
    cap_type = service["type"]

    if dry_run:
        print(f"  [dry-run] {cap_type:45s}  {name}")
        return "dry-run"

    body: dict[str, Any] = {
        "type": cap_type,
        "endpoint": service["endpoint"],
    }
    if service.get("params"):
        body["params"] = service["params"]

    try:
        response = client.post("/v1/publish", json=body)
        response.raise_for_status()
        data = response.json()
        descriptor_id = data.get("descriptor_id", "unknown")
        print(f"  [ok]      {cap_type:45s}  {name}  ->  {descriptor_id}")
        return descriptor_id
    except httpx.HTTPStatusError as exc:
        try:
            detail = exc.response.json().get("error", exc.response.text)
        except Exception:
            detail = exc.response.text
        print(f"  [FAIL]    {cap_type:45s}  {name}  ({exc.response.status_code}: {detail})")
        return None
    except httpx.RequestError as exc:
        print(f"  [FAIL]    {cap_type:45s}  {name}  (connection error: {exc})")
        return None


def seed_source(
    client: httpx.Client | None,
    label: str,
    services: list[dict[str, Any]],
    *,
    dry_run: bool = False,
) -> tuple[int, int]:
    """Seed all services from one source module.

    Returns (published_count, failed_count).
    """
    print(f"\n{'=' * 72}")
    print(f"  {label} ({len(services)} services)")
    print(f"{'=' * 72}")

    published = 0
    failed = 0

    for i, service in enumerate(services):
        result = publish_descriptor(client, service, dry_run=dry_run)
        if result is not None:
            published += 1
        else:
            failed += 1
        # Pause between publishes to stay under per-publisher rate limits
        if not dry_run and i < len(services) - 1:
            time.sleep(7)  # ~9 per minute, under the 10/min limit

    return published, failed


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Seed the MARS mesh network with real-world services.",
    )
    parser.add_argument(
        "--gateway",
        default="http://localhost:3000",
        help="Mesh gateway URL (default: http://localhost:3000)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Preview what would be published without actually publishing",
    )
    args = parser.parse_args()

    gateway_url = args.gateway.rstrip("/")

    print(f"MARS Network Seeder")
    print(f"Gateway: {gateway_url}")
    if args.dry_run:
        print("Mode:    DRY RUN (nothing will be published)")
    print()

    # Check gateway health (skip in dry-run mode)
    if not args.dry_run:
        try:
            with httpx.Client(base_url=gateway_url, timeout=10.0) as probe:
                resp = probe.get("/health")
                resp.raise_for_status()
                health = resp.json()
                print(f"Gateway healthy: identity={health.get('identity', '?')}")
        except Exception as exc:
            print(f"ERROR: Cannot reach gateway at {gateway_url}: {exc}")
            print("Is the mesh gateway running? Start it with: cargo run -p mesh-gateway")
            sys.exit(1)

    sources = [
        ("Public APIs", PUBLIC_APIS),
        ("MCP Skills", MCP_SKILLS),
        ("LLM Endpoints", LLM_ENDPOINTS),
    ]

    total_published = 0
    total_failed = 0
    total_services = 0

    if args.dry_run:
        for label, services in sources:
            total_services += len(services)
            published, failed = seed_source(
                None, label, services, dry_run=True,
            )
            total_published += published
            total_failed += failed
    else:
        with httpx.Client(base_url=gateway_url, timeout=30.0) as client:
            for label, services in sources:
                total_services += len(services)
                published, failed = seed_source(
                    client, label, services, dry_run=False,
                )
                total_published += published
                total_failed += failed

    # Summary
    print(f"\n{'=' * 72}")
    print(f"  SUMMARY")
    print(f"{'=' * 72}")
    print(f"  Total services:  {total_services}")
    print(f"  Published:       {total_published}")
    print(f"  Failed:          {total_failed}")
    print(f"  Skipped:         {total_services - total_published - total_failed}")
    print()

    if total_failed > 0:
        print(f"WARNING: {total_failed} service(s) failed to publish.")
        sys.exit(1 if total_published == 0 else 0)


if __name__ == "__main__":
    main()
