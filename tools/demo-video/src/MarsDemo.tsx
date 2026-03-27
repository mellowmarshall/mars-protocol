import React from "react";
import { useVideoConfig, Sequence } from "remotion";
import { Terminal, TypedLine, OutputLine, Spacer, COLORS } from "./Terminal";

// ── Scene timing (in seconds) ────────────────────────────────────────

const SCENES = {
  title: { start: 0, dur: 3 },
  install: { start: 3, dur: 2.5 },
  connect: { start: 5.5, dur: 2 },
  discoverSearch: { start: 7.5, dur: 4 },
  discoverInference: { start: 11.5, dur: 4 },
  discoverMcp: { start: 15.5, dur: 3.5 },
  publish: { start: 19, dur: 4 },
  closing: { start: 23, dur: 5 },
};

export const MarsDemo: React.FC = () => {
  const { fps } = useVideoConfig();
  const f = (sec: number) => Math.round(sec * fps);

  return (
    <Terminal>
      {/* ── Title ── */}
      <Sequence from={f(SCENES.title.start)} durationInFrames={f(28)}>
        <OutputLine
          text="  MARS — Mesh Agent Routing Standard"
          color={COLORS.cyan}
          startFrame={0}
        />
        <OutputLine
          text="  Decentralized capability discovery for AI agents"
          color={COLORS.dimmed}
          startFrame={f(0.8)}
        />
      </Sequence>

      {/* ── pip install ── */}
      <Sequence from={f(SCENES.install.start)} durationInFrames={f(25)} layout="none">
        <Spacer />
        <TypedLine
          text="pip install mesh-protocol"
          prefix="$ "
          prefixColor={COLORS.green}
          startFrame={0}
          charsPerFrame={2}
        />
        <OutputLine
          text="Successfully installed mesh-protocol-0.1.0"
          color={COLORS.dimmed}
          startFrame={f(1.2)}
        />
      </Sequence>

      {/* ── Connect ── */}
      <Sequence from={f(SCENES.connect.start)} durationInFrames={f(22)} layout="none">
        <Spacer />
        <TypedLine
          text='from mesh_protocol import MeshClient'
          prefix=">>> "
          prefixColor={COLORS.green}
          startFrame={0}
        />
        <TypedLine
          text='client = MeshClient("http://localhost:3000")'
          prefix=">>> "
          prefixColor={COLORS.green}
          startFrame={f(1)}
        />
      </Sequence>

      {/* ── Discover Search ── */}
      <Sequence from={f(SCENES.discoverSearch.start)} durationInFrames={f(20)} layout="none">
        <Spacer />
        <OutputLine text="# What search tools are on the mesh?" color={COLORS.dimmed} startFrame={0} />
        <TypedLine
          text='results = client.discover("data/search")'
          prefix=">>> "
          prefixColor={COLORS.green}
          startFrame={f(0.5)}
        />
        <OutputLine text="  data/search/ai          Tavily" color={COLORS.yellow} startFrame={f(2)} />
        <OutputLine text="  data/search/ai          Exa" color={COLORS.yellow} startFrame={f(2.3)} />
        <OutputLine text="  data/search/serp        Serper" color={COLORS.yellow} startFrame={f(2.6)} />
        <OutputLine text="  data/search/web         DuckDuckGo" color={COLORS.yellow} startFrame={f(2.9)} />
      </Sequence>

      {/* ── Discover Inference ── */}
      <Sequence from={f(SCENES.discoverInference.start)} durationInFrames={f(20)} layout="none">
        <Spacer />
        <OutputLine text="# What about LLM inference?" color={COLORS.dimmed} startFrame={0} />
        <TypedLine
          text='results = client.discover("compute/inference")'
          prefix=">>> "
          prefixColor={COLORS.green}
          startFrame={f(0.5)}
        />
        <OutputLine text="  compute/inference/text-generation    GLM-5 (HuggingFace)" color={COLORS.yellow} startFrame={f(2)} />
        <OutputLine text="  compute/inference/text-generation    OpenRouter" color={COLORS.yellow} startFrame={f(2.3)} />
        <OutputLine text="  compute/inference/text-generation    Groq" color={COLORS.yellow} startFrame={f(2.6)} />
        <OutputLine text="  compute/inference/image-generation   fal.ai" color={COLORS.yellow} startFrame={f(2.9)} />
        <OutputLine text="  ... and 7 more" color={COLORS.dimmed} startFrame={f(3.2)} />
      </Sequence>

      {/* ── Discover MCP ── */}
      <Sequence from={f(SCENES.discoverMcp.start)} durationInFrames={f(17)} layout="none">
        <Spacer />
        <OutputLine text="# MCP tools?" color={COLORS.dimmed} startFrame={0} />
        <TypedLine
          text='results = client.discover("mcp/tool")'
          prefix=">>> "
          prefixColor={COLORS.green}
          startFrame={f(0.5)}
        />
        <OutputLine
          text="  15 MCP tools: GitHub, Playwright, Postgres, Slack, Brave Search, ..."
          color={COLORS.yellow}
          startFrame={f(2)}
        />
      </Sequence>

      {/* ── Publish ── */}
      <Sequence from={f(SCENES.publish.start)} durationInFrames={f(20)} layout="none">
        <Spacer />
        <OutputLine text="# Publish my own capability" color={COLORS.dimmed} startFrame={0} />
        <TypedLine
          text='client.publish("compute/analysis/code-review",'
          prefix=">>> "
          prefixColor={COLORS.green}
          startFrame={f(0.5)}
        />
        <TypedLine
          text='    endpoint="https://my-agent.dev/review")'
          prefix="... "
          prefixColor={COLORS.green}
          startFrame={f(1.5)}
        />
        <OutputLine
          text="  ✓ Published — discoverable by any agent on the mesh"
          color={COLORS.green}
          startFrame={f(2.8)}
        />
      </Sequence>

      {/* ── Closing ── */}
      <Sequence from={f(SCENES.closing.start)} durationInFrames={f(5)} layout="none">
        <Spacer />
        <OutputLine
          text="  No config files. No central registry. No API keys."
          color={"#ffffff"}
          startFrame={0}
        />
        <OutputLine
          text="  Just a DHT."
          color={"#ffffff"}
          startFrame={f(1)}
        />
        <Spacer />
        <OutputLine
          text="  github.com/mellowmarshall/mars-protocol"
          color={COLORS.cyan}
          startFrame={f(2)}
        />
        <OutputLine
          text="  pip install mesh-protocol"
          color={COLORS.magenta}
          startFrame={f(2.8)}
        />
      </Sequence>
    </Terminal>
  );
};
