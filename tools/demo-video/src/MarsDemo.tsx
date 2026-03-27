import React from "react";
import { useCurrentFrame, useVideoConfig } from "remotion";
import { Terminal, TypedLine, OutputLine, Spacer, COLORS } from "./Terminal";

// ── Line data with timing ────────────────────────────────────────────
// Each line has: when it appears (seconds), type, and content.

interface Line {
  at: number; // seconds when this line appears
  type: "output" | "typed" | "spacer";
  text?: string;
  color?: string;
  prefix?: string;
  prefixColor?: string;
  charsPerFrame?: number;
}

const LINES: Line[] = [
  // Title
  { at: 0, type: "output", text: "  MARS — Mesh Agent Routing Standard", color: COLORS.cyan },
  { at: 0.8, type: "output", text: "  Decentralized capability discovery for AI agents", color: COLORS.dimmed },

  // Install
  { at: 2.5, type: "spacer" },
  { at: 2.5, type: "typed", text: "pip install mesh-protocol", prefix: "$ ", prefixColor: COLORS.green, charsPerFrame: 2 },
  { at: 3.8, type: "output", text: "Successfully installed mesh-protocol-0.1.0", color: COLORS.dimmed },

  // Connect
  { at: 5, type: "spacer" },
  { at: 5, type: "typed", text: "from mesh_protocol import MeshClient", prefix: ">>> ", prefixColor: COLORS.green },
  { at: 6, type: "typed", text: 'client = MeshClient("http://localhost:3000")', prefix: ">>> ", prefixColor: COLORS.green },

  // Discover Search
  { at: 7.5, type: "spacer" },
  { at: 7.5, type: "output", text: "# What search tools are on the mesh?", color: COLORS.dimmed },
  { at: 8, type: "typed", text: 'results = client.discover("data/search")', prefix: ">>> ", prefixColor: COLORS.green },
  { at: 9.5, type: "output", text: "  data/search/ai          Tavily", color: COLORS.yellow },
  { at: 9.8, type: "output", text: "  data/search/ai          Exa", color: COLORS.yellow },
  { at: 10.1, type: "output", text: "  data/search/serp        Serper", color: COLORS.yellow },
  { at: 10.4, type: "output", text: "  data/search/web         DuckDuckGo", color: COLORS.yellow },

  // Discover Inference
  { at: 11.5, type: "spacer" },
  { at: 11.5, type: "output", text: "# LLM inference?", color: COLORS.dimmed },
  { at: 12, type: "typed", text: 'client.discover("compute/inference")', prefix: ">>> ", prefixColor: COLORS.green },
  { at: 13.2, type: "output", text: "  11 providers: GLM-5, OpenRouter, Groq, fal.ai, ...", color: COLORS.yellow },

  // Discover MCP
  { at: 14.5, type: "spacer" },
  { at: 14.5, type: "output", text: "# MCP tools?", color: COLORS.dimmed },
  { at: 15, type: "typed", text: 'client.discover("mcp/tool")', prefix: ">>> ", prefixColor: COLORS.green },
  { at: 16.2, type: "output", text: "  15 tools: GitHub, Playwright, Postgres, Slack, ...", color: COLORS.yellow },

  // Publish
  { at: 17.5, type: "spacer" },
  { at: 17.5, type: "output", text: "# Publish my own capability", color: COLORS.dimmed },
  { at: 18, type: "typed", text: 'client.publish("compute/analysis/code-review",', prefix: ">>> ", prefixColor: COLORS.green },
  { at: 19, type: "typed", text: '    endpoint="https://my-agent.dev/review")', prefix: "... ", prefixColor: COLORS.green },
  { at: 20.5, type: "output", text: "  ✓ Published — discoverable by any agent on the mesh", color: COLORS.green },

  // Closing
  { at: 22, type: "spacer" },
  { at: 22, type: "spacer" },
  { at: 22, type: "output", text: "  No config files. No central registry. No API keys.", color: "#ffffff" },
  { at: 23, type: "output", text: "  Just a DHT.", color: "#ffffff" },
  { at: 24, type: "spacer" },
  { at: 24, type: "output", text: "  github.com/mellowmarshall/mars-protocol", color: COLORS.cyan },
  { at: 25, type: "output", text: "  pip install mesh-protocol", color: COLORS.magenta },
];

// ── Terminal height and scroll ───────────────────────────────────────

const LINE_HEIGHT = 32; // px per line
const VISIBLE_LINES = 18; // how many lines fit in the terminal
const VISIBLE_HEIGHT = VISIBLE_LINES * LINE_HEIGHT;

export const MarsDemo: React.FC = () => {
  const { fps } = useVideoConfig();
  const frame = useCurrentFrame();

  // Count how many lines are visible at the current frame
  const visibleCount = LINES.filter(
    (line) => frame >= Math.round(line.at * fps)
  ).length;

  // Scroll offset: once we exceed visible lines, scroll up
  const scrollLines = Math.max(0, visibleCount - VISIBLE_LINES);
  const targetScrollY = scrollLines * LINE_HEIGHT;

  return (
    <Terminal>
      <div
        style={{
          overflow: "hidden",
          height: VISIBLE_HEIGHT,
          position: "relative",
        }}
      >
        <div
          style={{
            transform: `translateY(-${targetScrollY}px)`,
          }}
        >
          {LINES.map((line, i) => {
            const lineFrame = Math.round(line.at * fps);

            if (line.type === "spacer") {
              return frame >= lineFrame ? <Spacer key={i} /> : null;
            }

            if (line.type === "typed") {
              return (
                <TypedLine
                  key={i}
                  text={line.text || ""}
                  prefix={line.prefix}
                  prefixColor={line.prefixColor}
                  color={line.color}
                  startFrame={lineFrame}
                  charsPerFrame={line.charsPerFrame}
                />
              );
            }

            // output
            return (
              <OutputLine
                key={i}
                text={line.text || ""}
                color={line.color}
                startFrame={lineFrame}
              />
            );
          })}
        </div>
      </div>
    </Terminal>
  );
};
