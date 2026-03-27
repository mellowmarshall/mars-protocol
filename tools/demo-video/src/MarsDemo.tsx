import React from "react";
import { useCurrentFrame, useVideoConfig, interpolate } from "remotion";
import { Terminal, TypedLine, OutputLine, Spacer, COLORS } from "./Terminal";
import { MiniTerminal, THEMES } from "./MiniTerminal";

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

const MONTAGE_START = 22;  // seconds — terminals start stacking
const CLOSING_START = 32;  // seconds — fade to MARS logo

// ── Montage: service connection terminals ────────────────────────────
// Each entry: [seconds_after_montage_start, theme_key, title, command, output_lines, x, y]
type MontageEntry = [number, string, string, string, string[], number, number];

const MONTAGE_TERMINALS: MontageEntry[] = [
  [0, "openclaw", "agent — search", 'client.discover("data/search/ai")', [
    "→ Tavily: AI-optimized web search",
    "→ Exa: Neural search engine",
  ], 50, 40],
  [1.2, "claude", "claude — inference", 'openai.chat.completions.create(model="glm-5")', [
    "→ Connected to GLM-5 (HuggingFace)",
    '→ Response: "The MARS protocol is..."',
  ], 200, 100],
  [2.2, "codex", "codex — code review", 'mesh.discover("compute/analysis/code-review")', [
    "→ Found 3 providers",
    "→ Calling my-agent.dev/review...",
  ], 80, 180],
  [3.0, "openclaw", "agent — scraping", 'firecrawl.scrape("https://mars-protocol.dev")', [
    "→ Firecrawl: 500 credits/mo free",
    "→ Scraping... 2.3s",
  ], 350, 60],
  [3.6, "claude", "claude — image gen", 'fal.run("flux.1-dev", prompt="mesh network")', [
    "→ fal.ai: FLUX.1 image generation",
    "→ Generated 1024x1024 in 4.1s",
  ], 150, 220],
  [4.1, "codex", "codex — sandbox", 'e2b.sandbox.run("python3 train.py")', [
    "→ E2B: Cloud code execution",
    "→ Sandbox ready (0.3s)",
  ], 420, 150],
  [4.5, "openclaw", "agent — database", 'neon.query("SELECT * FROM agents LIMIT 10")', [
    "→ Neon Serverless Postgres",
    "→ 10 rows (12ms)",
  ], 100, 300],
  [4.8, "claude", "claude — MCP tools", 'mcp.call_tool("github", {repo: "mars-protocol"})', [
    "→ 15 MCP tools available",
    "→ GitHub: 42 stars, 7 forks",
  ], 300, 280],
  [5.0, "codex", "codex — embeddings", 'embed("MARS protocol for agent discovery")', [
    "→ BGE Large: 1024-dim vector",
  ], 500, 100],
  [5.2, "openclaw", "agent — translation", 'translate("Hello", target="ja")', [
    "→ NLLB: こんにちは",
  ], 50, 380],
  [5.3, "claude", "claude — weather", 'open_meteo.forecast(lat=40.7, lon=-74.0)', [
    "→ NYC: 24°C, sunny",
  ], 450, 330],
  [5.4, "codex", "codex — speech", 'whisper.transcribe("meeting.mp3")', [
    "→ Whisper v3 Turbo",
  ], 250, 380],
];

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

      {/* ── Montage: stacking terminal windows ── */}
      {frame >= Math.round(MONTAGE_START * fps) && MONTAGE_TERMINALS.map(([delay, themeKey, title, command, output, mx, my], i) => (
        <MiniTerminal
          key={`montage-${i}`}
          theme={THEMES[themeKey]}
          title={title}
          command={command}
          output={output}
          startFrame={Math.round((MONTAGE_START + delay) * fps)}
          x={mx}
          y={my}
          width={560}
          height={240}
        />
      ))}

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
