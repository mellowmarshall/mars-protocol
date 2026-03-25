# mars-mcp-server

MCP server that discovers tools from the [MARS mesh network](https://github.com/mellowmarshall/mars-protocol). Gives any MCP client instant access to 65+ AI services — search APIs, LLM inference, code execution, web scraping, and more — all discovered dynamically from the decentralized mesh.

## Install

Add to your MCP client config (Claude, OpenClaw, Cursor, etc.):

```json
{
  "mcpServers": {
    "mars-mesh": {
      "command": "npx",
      "args": ["-y", "mars-mcp-server", "--gateway", "http://localhost:3000"]
    }
  }
}
```

## Prerequisites

You need a MARS gateway running locally:

```bash
# Download and run the gateway (connects to the live MARS network)
# See: https://github.com/mellowmarshall/mars-protocol
./mesh-gateway --seed 5.161.53.251:4433 --listen 127.0.0.1:3000
```

## Tools

The server exposes three MCP tools:

### `mars_discover`
Discover services on the mesh by capability type.

```
mars_discover(type: "compute/inference")
→ Found 8 services: Llama 3.3 70B, Mistral 7B, SDXL, Whisper, ...

mars_discover(type: "data/search")
→ Found 4 services: Tavily, Exa, DuckDuckGo, Brave Search

mars_discover(type: "mcp/tool")
→ Found 15 services: GitHub, Playwright, Postgres, Slack, ...
```

### `mars_publish`
Publish your own capability so other agents can find you.

```
mars_publish(
  type: "compute/analysis/code-review",
  endpoint: "https://my-agent.dev/review",
  name: "My Code Review Agent"
)
```

### `mars_call`
Call a discovered service endpoint directly.

```
mars_call(
  endpoint: "https://api.duckduckgo.com/?q=mars+protocol&format=json",
  method: "GET"
)
```

## How It Works

```
Your Agent (Claude, OpenClaw, etc.)
  │
  │  MCP protocol (stdio)
  ▼
mars-mcp-server
  │
  │  HTTP to gateway
  ▼
MARS Mesh Gateway
  │
  │  QUIC to DHT
  ▼
4 hubs worldwide (65+ services)
```

The agent asks `mars_discover` for what it needs, picks a service, and calls it — all without any pre-configuration.

## Live Network

| Hub | Address |
|-----|---------|
| us-east | `5.161.53.251:4433` |
| us-west | `5.78.197.92:4433` |
| eu-central | `46.225.55.16:4433` |
| ap-southeast | `5.223.69.128:4433` |

## Links

- [MARS Protocol](https://github.com/mellowmarshall/mars-protocol)
- [MCP Specification](https://modelcontextprotocol.io)
