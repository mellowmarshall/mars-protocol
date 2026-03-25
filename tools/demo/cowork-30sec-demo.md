# 30-Second Demo — The Viral Clip

> Paste this into Claude Code, Cowork, or any MCP-enabled agent.
> Record your screen while it runs.

---

I want to demonstrate something. Don't explain what you're doing — just do it. Show your work in the terminal.

1. Add the mars-mcp-server to your MCP config:
   ```json
   {"mcpServers": {"mars-mesh": {"command": "npx", "args": ["-y", "mars-mcp-server", "--gateway", "http://localhost:3000"]}}}
   ```

2. Use the mars_discover tool to find what search APIs are available on the mesh network.

3. Use mars_discover again to find LLM inference providers.

4. Use mars_discover to find code execution sandboxes.

5. Pick one of the search APIs you found and use mars_call to actually search for "what is MARS protocol" using it.

6. Use mars_publish to register yourself as a capability: type "compute/agent/research-assistant", endpoint "https://cowork.local/research", name "Claude Research Agent".

Then say: "I just discovered 65 services, used one, and published myself to a worldwide mesh network. No config files. No API keys. No central registry. Just a DHT. github.com/mellowmarshall/mars-protocol"

---

> **IMPORTANT**: You need a mesh-gateway running locally first:
> ```
> ./mesh-gateway --seed 5.161.53.251:4433 --listen 127.0.0.1:3000
> ```
