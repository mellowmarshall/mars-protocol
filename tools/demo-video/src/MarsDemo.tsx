import React from "react";
import { useCurrentFrame, useVideoConfig, interpolate } from "remotion";
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
  { at: 2.5, type: "typed", text: "pip install mesh-protocol", prefix: "$ ", prefixColor: COLORS.orange, charsPerFrame: 2 },
  { at: 3.8, type: "output", text: "Successfully installed mesh-protocol-0.1.0", color: COLORS.dimmed },

  // Connect
  { at: 5, type: "spacer" },
  { at: 5, type: "typed", text: "from mesh_protocol import MeshClient", prefix: "🦞 ", prefixColor: COLORS.orange },
  { at: 6, type: "typed", text: 'client = MeshClient("http://localhost:3000")', prefix: "🦞 ", prefixColor: COLORS.orange },

  // Discover Search
  { at: 7.5, type: "spacer" },
  { at: 7.5, type: "output", text: "# What search tools are on the mesh?", color: COLORS.dimmed },
  { at: 8, type: "typed", text: 'results = client.discover("data/search")', prefix: "🦞 ", prefixColor: COLORS.orange },
  { at: 9.5, type: "output", text: "  data/search/ai          Tavily", color: COLORS.yellow },
  { at: 9.8, type: "output", text: "  data/search/ai          Exa", color: COLORS.yellow },
  { at: 10.1, type: "output", text: "  data/search/serp        Serper", color: COLORS.yellow },
  { at: 10.4, type: "output", text: "  data/search/web         DuckDuckGo", color: COLORS.yellow },

  // Discover Inference
  { at: 11.5, type: "spacer" },
  { at: 11.5, type: "output", text: "# LLM inference?", color: COLORS.dimmed },
  { at: 12, type: "typed", text: 'client.discover("compute/inference")', prefix: "🦞 ", prefixColor: COLORS.orange },
  { at: 13.2, type: "output", text: "  11 providers: GLM-5, OpenRouter, Groq, fal.ai, ...", color: COLORS.yellow },

  // Discover MCP
  { at: 14.5, type: "spacer" },
  { at: 14.5, type: "output", text: "# MCP tools?", color: COLORS.dimmed },
  { at: 15, type: "typed", text: 'client.discover("mcp/tool")', prefix: "🦞 ", prefixColor: COLORS.orange },
  { at: 16.2, type: "output", text: "  15 tools: GitHub, Playwright, Postgres, Slack, ...", color: COLORS.yellow },

  // Publish
  { at: 17.5, type: "spacer" },
  { at: 17.5, type: "output", text: "# Publish my own capability", color: COLORS.dimmed },
  { at: 18, type: "typed", text: 'client.publish("compute/analysis/code-review",', prefix: "🦞 ", prefixColor: COLORS.orange },
  { at: 19, type: "typed", text: '    endpoint="https://my-agent.dev/review")', prefix: "   ", prefixColor: COLORS.orange },
  { at: 20.5, type: "output", text: "  ✓ Published — discoverable by any agent on the mesh", color: COLORS.green },

];

const CLOSING_START = 22; // seconds — when the fade begins

const ASCII_MARS = `
 ███╗   ███╗  █████╗  ██████╗  ███████╗
 ████╗ ████║ ██╔══██╗ ██╔══██╗ ██╔════╝
 ██╔████╔██║ ███████║ ██████╔╝ ███████╗
 ██║╚██╔╝██║ ██╔══██║ ██╔══██╗ ╚════██║
 ██║ ╚═╝ ██║ ██║  ██║ ██║  ██║ ███████║
 ╚═╝     ╚═╝ ╚═╝  ╚═╝ ╚═╝  ╚═╝ ╚══════╝`.trim();

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

      {/* ── Closing: fade overlay with ASCII MARS ── */}
      {frame >= Math.round(CLOSING_START * fps) && (() => {
        const closingFrame = frame - Math.round(CLOSING_START * fps);
        const bgOpacity = interpolate(closingFrame, [0, Math.round(1.5 * fps)], [0, 1], {
          extrapolateRight: "clamp",
        });
        const titleOpacity = interpolate(closingFrame, [Math.round(1 * fps), Math.round(2 * fps)], [0, 1], {
          extrapolateLeft: "clamp", extrapolateRight: "clamp",
        });
        const asciiOpacity = interpolate(closingFrame, [Math.round(2 * fps), Math.round(3 * fps)], [0, 1], {
          extrapolateLeft: "clamp", extrapolateRight: "clamp",
        });
        const subtitleOpacity = interpolate(closingFrame, [Math.round(3 * fps), Math.round(4 * fps)], [0, 1], {
          extrapolateLeft: "clamp", extrapolateRight: "clamp",
        });

        return (
          <div
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              right: 0,
              bottom: 0,
              backgroundColor: COLORS.bg,
              opacity: bgOpacity,
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              justifyContent: "center",
              fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
            }}
          >
            <div style={{ opacity: titleOpacity, fontSize: 18, color: COLORS.dimmed, letterSpacing: "0.15em", marginBottom: 16 }}>
              Mesh Agent Routing Standard
            </div>
            <pre style={{ opacity: asciiOpacity, color: COLORS.orange, fontSize: 16, lineHeight: 1.2, textAlign: "center", margin: 0 }}>
              {ASCII_MARS}
            </pre>
            <div style={{ opacity: subtitleOpacity, fontSize: 18, color: COLORS.dimmed, letterSpacing: "0.15em", marginTop: 16 }}>
              Protocol
            </div>
          </div>
        );
      })()}
    </Terminal>
  );
};
