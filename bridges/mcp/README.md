# mesh-mcp-bridge

Bidirectional bridge between [MCP](https://modelcontextprotocol.io/) servers and the [mesh protocol](https://github.com/marshallbrett/mesh-protocol) network.
It publishes MCP server capabilities as mesh descriptors **and** exposes mesh capabilities to MCP-compatible agents as standard MCP tools.

## Install

```
pip install mesh-mcp-bridge
```

## Usage

### Publish MCP server tools to the mesh

```bash
mesh-mcp-bridge publish \
  --gateway http://localhost:3000 \
  --mcp-server "python my_mcp_server.py" \
  --name "my-tools"
```

This connects to the MCP server, lists its tools, and registers each one as a
mesh descriptor (type `mcp/tool/{tool_name}`).

### Discover mesh capabilities via MCP

```bash
mesh-mcp-bridge serve \
  --gateway http://localhost:3000 \
  --transport stdio
```

Or over HTTP:

```bash
mesh-mcp-bridge serve \
  --gateway http://localhost:3000 \
  --transport http \
  --port 8080
```

Any MCP-compatible agent can then connect to this server and see all
`mcp/tool/*` descriptors on the mesh as standard MCP tools.

## Architecture

```
┌──────────────┐         ┌──────────────────┐         ┌──────────────┐
│  MCP Server  │◄───────►│  mesh-mcp-bridge │◄───────►│ Mesh Gateway │
│  (tools)     │  stdio  │                  │  HTTP   │  /v1/publish │
└──────────────┘         │  publish mode:   │         │  /v1/discover│
                         │  MCP → mesh      │         └──────┬───────┘
                         │                  │                │
                         │  serve mode:     │                ▼
┌──────────────┐         │  mesh → MCP      │         ┌──────────────┐
│  MCP Client  │◄───────►│                  │         │  Mesh DHT    │
│  (agent)     │ stdio/  └──────────────────┘         │  Network     │
└──────────────┘  HTTP                                └──────────────┘
```

**Publish mode** — scans an MCP server's tool listing and registers each tool
as a mesh descriptor so any mesh participant can discover it.

**Serve mode** — queries the mesh for `mcp/tool/*` descriptors and presents
them as MCP tools.  When an agent invokes a tool the bridge forwards the call
to the endpoint recorded in the descriptor.

## Links

- [mesh-protocol](https://github.com/marshallbrett/mesh-protocol)
- [Model Context Protocol](https://modelcontextprotocol.io/)
- [MCP Python SDK](https://github.com/modelcontextprotocol/python-sdk)
