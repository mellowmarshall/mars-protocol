# Cowork Prompt — The Viral Demo

> This is the demo that makes people stop scrolling.
> An AI agent discovers its own tools through the mesh — no config, no hardcoding.
>
> 1. Paste the OBS SETUP section first, let Cowork configure recording
> 2. Then paste the DEMO section and let it rip

---

## Part 1: OBS SETUP (paste this first)

Install OBS Studio if it's not installed (winget install OBSProject.OBSStudio).
Then open OBS, configure it to record my screen at 1280x720 to C:\Users\logan\Videos as mp4.
Enable the WebSocket server on port 4455 with no authentication.
Add a Display Capture source so it records my whole screen.
Install obs-cmd (pip install obs-cmd) so you can start/stop recording from the command line.
Let me know when OBS is ready.

---

## Part 2: THE DEMO (paste this after OBS is ready)

Start OBS recording with obs-cmd, then do this in a visible terminal:

You are an AI agent that needs to complete a task, but you don't have
any tools configured. Use the MARS mesh network to discover what you
need dynamically.

First, start the mesh gateway in a WSL terminal:
```
cd /home/logan/Dev/mesh-protocol && ./target/release/mesh-gateway --seed 5.161.53.251:4433 --listen 127.0.0.1:3000 &
sleep 4
```

Now open a Python session and do this step by step, explaining what
you're doing out loud (type comments in the terminal):

```python
# I'm an AI agent. I need to research a topic, but I don't have any
# search tools configured. Let me ask the mesh network what's available.

from mesh_protocol import MeshClient
client = MeshClient("http://localhost:3000")

# What search tools exist on the mesh?
search_tools = client.discover("data/search")
for tool in search_tools:
    print(f"Found: {tool.type} — {tool.params.get('name')} ({tool.endpoint})")

# Nice — I found Tavily, Exa, and DuckDuckGo. No config files needed.
# What about if I need to generate images?
image_tools = client.discover("compute/inference/image-generation")
for tool in image_tools:
    print(f"Found: {tool.type} — {tool.params.get('name')}")

# What about code execution sandboxes?
sandboxes = client.discover("compute/sandbox")
for tool in sandboxes:
    print(f"Found: {tool.type} — {tool.params.get('name')}")

# I can also discover MCP tools for deeper integration
mcp_tools = client.discover("mcp/tool")
print(f"\n{len(mcp_tools)} MCP tools available on the mesh")
for tool in mcp_tools:
    print(f"  {tool.params.get('name')}: {tool.params.get('description', '')}")

# Now let me publish MY OWN capability so other agents can find me
result = client.publish(
    "compute/analysis/research-agent",
    endpoint="https://cowork.local/research",
    params={"name": "Cowork Research Agent", "capabilities": ["web research", "summarization", "fact checking"]}
)
print(f"\nPublished myself to the mesh: {result.descriptor_id}")
print("Any agent in the world can now discover me.")
```

After the Python session completes, type this final comment:
```
# No API keys configured. No config files. No central registry.
# The agent discovered 10+ tools and published itself in 30 seconds.
# github.com/mellowmarshall/mars-protocol
```

Wait 3 seconds, then stop OBS recording with obs-cmd.

Tell me the filename of the recording when done.
