import React from "react";
import { useCurrentFrame, interpolate } from "remotion";

// ── Theme variants ───────────────────────────────────────────────────

export interface TerminalTheme {
  name: string;
  bg: string;
  fg: string;
  accent: string;
  dimmed: string;
  titleBg: string;
  titleFg: string;
  icon: string;
}

export const THEMES: Record<string, TerminalTheme> = {
  openclaw: {
    name: "OpenClaw",
    bg: "#1a1b1e",
    fg: "#c9cdd6",
    accent: "#FF5A2D",
    dimmed: "#565f89",
    titleBg: "#252529",
    titleFg: "#FF5A2D",
    icon: "🦞",
  },
  claude: {
    name: "Claude Code",
    bg: "#1b1a2e",
    fg: "#d4d0f0",
    accent: "#d4a574",
    dimmed: "#6b6890",
    titleBg: "#252440",
    titleFg: "#d4a574",
    icon: "◈",
  },
  codex: {
    name: "Codex",
    bg: "#0d1117",
    fg: "#7ee787",
    accent: "#58a6ff",
    dimmed: "#484f58",
    titleBg: "#161b22",
    titleFg: "#58a6ff",
    icon: "▶",
  },
};

// ── Mini terminal component ──────────────────────────────────────────

interface MiniTerminalProps {
  theme: TerminalTheme;
  title: string;
  command: string;
  output: string[];
  startFrame: number;
  x: number;
  y: number;
  width?: number;
  height?: number;
}

export const MiniTerminal: React.FC<MiniTerminalProps> = ({
  theme,
  title,
  command,
  output,
  startFrame,
  x,
  y,
  width = 600,
  height = 280,
}) => {
  const frame = useCurrentFrame();
  const elapsed = frame - startFrame;

  if (elapsed < 0) return null;

  // Slide in + fade in
  const opacity = interpolate(elapsed, [0, 8], [0, 1], { extrapolateRight: "clamp" });
  const slideY = interpolate(elapsed, [0, 8], [30, 0], { extrapolateRight: "clamp" });

  // Typing effect for command
  const charsVisible = Math.min(Math.floor(elapsed * 1.5), command.length);

  // Output lines appear staggered
  const outputDelay = Math.round(command.length / 1.5) + 5;

  return (
    <div
      style={{
        position: "absolute",
        left: x,
        top: y + slideY,
        width,
        height,
        opacity,
        borderRadius: 10,
        overflow: "hidden",
        boxShadow: `0 8px 32px rgba(0,0,0,0.5)`,
        border: `1px solid ${theme.dimmed}33`,
        fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
      }}
    >
      {/* Title bar */}
      <div
        style={{
          backgroundColor: theme.titleBg,
          padding: "8px 14px",
          display: "flex",
          alignItems: "center",
          gap: 8,
          borderBottom: `1px solid ${theme.dimmed}22`,
        }}
      >
        <div style={{ display: "flex", gap: 6 }}>
          <div style={{ width: 10, height: 10, borderRadius: "50%", backgroundColor: "#ff5f56" }} />
          <div style={{ width: 10, height: 10, borderRadius: "50%", backgroundColor: "#ffbd2e" }} />
          <div style={{ width: 10, height: 10, borderRadius: "50%", backgroundColor: "#27c93f" }} />
        </div>
        <span style={{ color: theme.titleFg, fontSize: 11, marginLeft: 8 }}>
          {theme.icon} {title}
        </span>
      </div>

      {/* Body */}
      <div
        style={{
          backgroundColor: theme.bg,
          padding: "12px 16px",
          height: height - 36,
          fontSize: 13,
          lineHeight: 1.6,
          color: theme.fg,
        }}
      >
        {/* Command */}
        <div style={{ whiteSpace: "pre" }}>
          <span style={{ color: theme.accent }}>❯ </span>
          <span>{command.slice(0, charsVisible)}</span>
          {charsVisible < command.length && (
            <span style={{ backgroundColor: theme.fg, color: theme.bg }}> </span>
          )}
        </div>

        {/* Output */}
        {output.map((line, i) => {
          const lineVisible = elapsed > outputDelay + i * 4;
          if (!lineVisible) return null;
          const lineOpacity = interpolate(
            elapsed - outputDelay - i * 4,
            [0, 4],
            [0, 1],
            { extrapolateRight: "clamp" }
          );
          return (
            <div key={i} style={{ whiteSpace: "pre", opacity: lineOpacity, color: theme.dimmed }}>
              {line}
            </div>
          );
        })}
      </div>
    </div>
  );
};
