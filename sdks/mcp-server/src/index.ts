#!/usr/bin/env node
/**
 * mars-mcp-server — MCP server that discovers tools from the MARS mesh network.
 *
 * Any MCP client (Claude, OpenClaw, CrewAI) connects to this server and
 * instantly gets access to 65+ services: search APIs, LLM inference,
 * code execution, web scraping, and more — all discovered dynamically
 * from the decentralized mesh.
 *
 * Usage:
 *   npx mars-mcp-server --gateway http://localhost:3000
 *
 * In MCP config:
 *   {
 *     "mcpServers": {
 *       "mars-mesh": {
 *         "command": "npx",
 *         "args": ["-y", "mars-mcp-server", "--gateway", "http://localhost:3000"]
 *       }
 *     }
 *   }
 */

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

// ── Config ────────────────────────────────────────────────────────────

const args = process.argv.slice(2);
let gatewayUrl = "http://localhost:3000";

for (let i = 0; i < args.length; i++) {
  if (args[i] === "--gateway" && args[i + 1]) {
    gatewayUrl = args[i + 1];
    i++;
  }
}

const GATEWAY = gatewayUrl.replace(/\/+$/, "");
const CACHE_TTL_MS = 60_000; // 60s cache for discovery results

// ── Types ─────────────────────────────────────────────────────────────

interface Descriptor {
  id: string;
  publisher: string;
  type: string;
  endpoint: string;
  params?: Record<string, unknown>;
  timestamp: number;
  ttl: number;
  sequence: number;
}

interface DiscoverResponse {
  descriptors: Descriptor[];
}

// ── Discovery Cache ───────────────────────────────────────────────────

let cachedTools: Descriptor[] = [];
let cacheTimestamp = 0;

async function discoverAll(): Promise<Descriptor[]> {
  const now = Date.now();
  if (cachedTools.length > 0 && now - cacheTimestamp < CACHE_TTL_MS) {
    return cachedTools;
  }

  // Discover from multiple categories in parallel
  const categories = [
    "data/search",
    "data/scraping",
    "compute/inference",
    "compute/sandbox",
    "compute/database",
    "compute/messaging",
    "compute/document",
    "compute/observability",
    "mcp/tool",
  ];

  const results = await Promise.allSettled(
    categories.map(async (cat) => {
      const url = `${GATEWAY}/v1/discover?type=${encodeURIComponent(cat)}`;
      const resp = await fetch(url, { signal: AbortSignal.timeout(10_000) });
      if (!resp.ok) return [];
      const data = (await resp.json()) as DiscoverResponse;
      return data.descriptors || [];
    })
  );

  // Deduplicate by descriptor ID
  const seen = new Set<string>();
  const all: Descriptor[] = [];
  for (const result of results) {
    if (result.status === "fulfilled") {
      for (const d of result.value) {
        if (!seen.has(d.id)) {
          seen.add(d.id);
          all.push(d);
        }
      }
    }
  }

  cachedTools = all;
  cacheTimestamp = now;
  return all;
}

// ── Tool name helpers ─────────────────────────────────────────────────

function descriptorToToolName(d: Descriptor): string {
  // Convert type path to a valid MCP tool name: "data/search/ai" → "mars_data_search_ai"
  return "mars_" + d.type.replace(/\//g, "_").replace(/[^a-zA-Z0-9_]/g, "");
}

function toolNameToType(name: string): string {
  // Reverse: "mars_data_search_ai" → "data/search/ai"
  return name.replace(/^mars_/, "").replace(/_/g, "/");
}

// ── MCP Server ────────────────────────────────────────────────────────

const server = new McpServer({
  name: "mars-mesh",
  version: "0.1.0",
});

// Dynamic tool registration — discover from mesh and register each as a tool
server.tool(
  "mars_discover",
  "Discover capabilities on the MARS mesh network. Returns available services matching the given type prefix.",
  { type: z.string().describe("Capability type to search for (e.g. 'compute/inference', 'data/search', 'mcp/tool')") },
  async ({ type }) => {
    try {
      const url = `${GATEWAY}/v1/discover?type=${encodeURIComponent(type)}`;
      const resp = await fetch(url, { signal: AbortSignal.timeout(10_000) });
      if (!resp.ok) {
        return { content: [{ type: "text" as const, text: `Discovery failed: HTTP ${resp.status}` }] };
      }
      const data = (await resp.json()) as DiscoverResponse;
      const descriptors = data.descriptors || [];

      if (descriptors.length === 0) {
        return { content: [{ type: "text" as const, text: `No services found for type: ${type}` }] };
      }

      const lines = descriptors.map((d) => {
        const name = (d.params as Record<string, unknown>)?.name ?? d.endpoint;
        const desc = (d.params as Record<string, unknown>)?.description ?? "";
        return `- **${d.type}**: ${name}\n  Endpoint: ${d.endpoint}\n  ${desc}`;
      });

      return {
        content: [{
          type: "text" as const,
          text: `Found ${descriptors.length} service(s) on the MARS mesh:\n\n${lines.join("\n\n")}`,
        }],
      };
    } catch (e) {
      return { content: [{ type: "text" as const, text: `Discovery error: ${e}` }] };
    }
  }
);

server.tool(
  "mars_publish",
  "Publish a capability to the MARS mesh network so other agents can discover it.",
  {
    type: z.string().describe("Capability type (e.g. 'compute/inference/text-generation')"),
    endpoint: z.string().describe("URL where the capability can be invoked"),
    name: z.string().optional().describe("Human-readable name for this capability"),
    description: z.string().optional().describe("Description of what this capability does"),
  },
  async ({ type, endpoint, name, description }) => {
    try {
      const body: Record<string, unknown> = { type, endpoint };
      if (name || description) {
        body.params = { ...(name && { name }), ...(description && { description }) };
      }

      const resp = await fetch(`${GATEWAY}/v1/publish`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(10_000),
      });

      if (!resp.ok) {
        const text = await resp.text();
        return { content: [{ type: "text" as const, text: `Publish failed: ${text}` }] };
      }

      const data = await resp.json() as { descriptor_id: string };
      return {
        content: [{
          type: "text" as const,
          text: `Published to MARS mesh!\nDescriptor ID: ${data.descriptor_id}\nType: ${type}\nEndpoint: ${endpoint}`,
        }],
      };
    } catch (e) {
      return { content: [{ type: "text" as const, text: `Publish error: ${e}` }] };
    }
  }
);

server.tool(
  "mars_call",
  "Call a service endpoint discovered on the MARS mesh. Sends a POST request with the given arguments.",
  {
    endpoint: z.string().describe("The service endpoint URL (from discovery results)"),
    body: z.string().optional().describe("JSON body to send (optional)"),
    method: z.string().optional().describe("HTTP method (default: POST)"),
  },
  async ({ endpoint, body, method }) => {
    try {
      const httpMethod = method?.toUpperCase() ?? "POST";
      const opts: RequestInit = {
        method: httpMethod,
        signal: AbortSignal.timeout(30_000),
      };

      if (body && httpMethod !== "GET") {
        opts.headers = { "Content-Type": "application/json" };
        opts.body = body;
      }

      const resp = await fetch(endpoint, opts);
      const text = await resp.text();

      // Try to pretty-print JSON
      let formatted: string;
      try {
        formatted = JSON.stringify(JSON.parse(text), null, 2);
      } catch {
        formatted = text;
      }

      return {
        content: [{
          type: "text" as const,
          text: `${resp.status} ${resp.statusText}\n\n${formatted.slice(0, 4000)}`,
        }],
      };
    } catch (e) {
      return { content: [{ type: "text" as const, text: `Call error: ${e}` }] };
    }
  }
);

// ── Start ─────────────────────────────────────────────────────────────

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error(`mars-mcp-server connected (gateway: ${GATEWAY})`);
}

main().catch((e) => {
  console.error("Fatal:", e);
  process.exit(1);
});
