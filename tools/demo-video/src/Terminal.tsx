import React from "react";
import { useCurrentFrame, interpolate } from "remotion";

// ── Terminal chrome ──────────────────────────────────────────────────

const COLORS = {
  bg: "#1a1b26",
  fg: "#a9b1d6",
  green: "#9ece6a",
  cyan: "#7dcfff",
  yellow: "#e0af68",
  red: "#f7768e",
  magenta: "#bb9af7",
  dimmed: "#565f89",
  prompt: "#73daca",
};

export const Terminal: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  return (
    <div
      style={{
        width: "100%",
        height: "100%",
        backgroundColor: COLORS.bg,
        fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
        fontSize: 20,
        lineHeight: 1.6,
        color: COLORS.fg,
        padding: "60px 40px 40px 40px",
        boxSizing: "border-box",
        position: "relative",
        overflow: "hidden",
      }}
    >
      {/* Window buttons */}
      <div style={{ position: "absolute", top: 16, left: 20, display: "flex", gap: 8 }}>
        <div style={{ width: 14, height: 14, borderRadius: "50%", backgroundColor: "#ff5f56" }} />
        <div style={{ width: 14, height: 14, borderRadius: "50%", backgroundColor: "#ffbd2e" }} />
        <div style={{ width: 14, height: 14, borderRadius: "50%", backgroundColor: "#27c93f" }} />
      </div>
      <div
        style={{
          position: "absolute",
          top: 14,
          width: "100%",
          textAlign: "center",
          fontSize: 14,
          color: COLORS.dimmed,
          left: 0,
        }}
      >
        mars-protocol — demo
      </div>
      {children}
    </div>
  );
};

// ── Typewriter line ──────────────────────────────────────────────────

interface TypedLineProps {
  text: string;
  color?: string;
  startFrame: number;
  charsPerFrame?: number;
  prefix?: string;
  prefixColor?: string;
}

export const TypedLine: React.FC<TypedLineProps> = ({
  text,
  color = COLORS.fg,
  startFrame,
  charsPerFrame = 1.5,
  prefix = "",
  prefixColor = COLORS.prompt,
}) => {
  const frame = useCurrentFrame();
  const elapsed = frame - startFrame;

  if (elapsed < 0) return null;

  const charsVisible = Math.min(Math.floor(elapsed * charsPerFrame), text.length);
  const showCursor = elapsed < text.length / charsPerFrame + 10;

  return (
    <div style={{ whiteSpace: "pre", minHeight: "1.6em" }}>
      {prefix && <span style={{ color: prefixColor }}>{prefix}</span>}
      <span style={{ color }}>{text.slice(0, charsVisible)}</span>
      {showCursor && charsVisible < text.length && (
        <span
          style={{
            backgroundColor: COLORS.fg,
            color: COLORS.bg,
            animation: "none",
          }}
        >
          {" "}
        </span>
      )}
    </div>
  );
};

// ── Instant output line (appears all at once) ────────────────────────

interface OutputLineProps {
  text: string;
  color?: string;
  startFrame: number;
}

export const OutputLine: React.FC<OutputLineProps> = ({
  text,
  color = COLORS.fg,
  startFrame,
}) => {
  const frame = useCurrentFrame();
  if (frame < startFrame) return null;

  const opacity = interpolate(frame - startFrame, [0, 5], [0, 1], {
    extrapolateRight: "clamp",
  });

  return (
    <div style={{ whiteSpace: "pre", color, opacity, minHeight: "1.6em" }}>
      {text}
    </div>
  );
};

// ── Blank line spacer ────────────────────────────────────────────────

export const Spacer: React.FC = () => <div style={{ height: "1.6em" }} />;

export { COLORS };
