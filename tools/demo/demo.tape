# MARS Protocol Demo — Terminal Recording
# Generates demo.gif for Twitter/social sharing
#
# Run: vhs demo.tape

Output demo.gif
Output demo.mp4

Set FontSize 18
Set Width 900
Set Height 500
Set Theme "Catppuccin Mocha"
Set Padding 20
Set TypingSpeed 40ms

# Title
Type "# MARS — Mesh Agent Routing Standard"
Enter
Type "# Decentralized capability discovery for AI agents"
Enter
Enter
Sleep 1.5s

# Install
Type "pip install mesh-protocol"
Enter
Sleep 2s

# Start Python
Type "python3"
Enter
Sleep 500ms

# Import
Type "from mesh_protocol import MeshClient"
Enter
Sleep 500ms

# Connect
Type 'client = MeshClient("http://localhost:3000")'
Enter
Sleep 500ms

# Discover AI search
Type 'results = client.discover("data/search")'
Enter
Sleep 1s

Type "for r in results:"
Enter
Type '    print(f"  {r.type} → {r.endpoint[:50]}")'
Enter
Enter
Sleep 2s

# Discover inference
Type 'results = client.discover("compute/inference")'
Enter
Sleep 1s

Type "for r in results:"
Enter
Type '    print(f"  {r.type} → {r.params.get(\'name\', \'\')}")'
Enter
Enter
Sleep 2s

# Discover MCP tools
Type 'results = client.discover("mcp/tool")'
Enter
Sleep 1s

Type "print(f'{len(results)} MCP tools discoverable')"
Enter
Sleep 2s

# Publish
Type 'client.publish("compute/analysis/code-review",'
Enter
Type '    endpoint="https://my-agent.dev/review",'
Enter
Type '    params={"languages": ["rust", "python"]})'
Enter
Sleep 1.5s

# Final
Type "# No config files. No central registry."
Enter
Type "# Just a DHT. github.com/mellowmarshall/mars-protocol"
Enter
Sleep 3s
