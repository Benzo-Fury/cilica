/**
 * Accent presets (design-system README › Color › Accent presets).
 *
 * Picking a preset regenerates the four accent tokens in OKLCH from the preset's hue, keeping the
 * lightness and chroma of the default amber:
 *   accent       oklch(0.76 0.165 h) light · oklch(0.77 0.155 h) dark
 *   accent-hover 0.04 darker in light, 0.04 lighter in dark
 *   accent-text  oklch(0.52 0.14 h)  light · oklch(0.81 0.13 h)  dark
 *   accent-soft  oklch(0.955 0.035 h) light · oklch(0.295 0.055 h) dark
 * `on-accent` is re-derived so it keeps its contrast; Graphite is neutral and uses `bg-canvas`.
 */
import type { AccentPreset } from "@/ipc/types";

export type ThemeName = "light" | "dark";
export type AccentToken = "accent" | "accent-hover" | "accent-text" | "accent-soft" | "on-accent";
export type AccentTokens = Record<AccentToken, string>;

/** Hue (OKLCH degrees) of each preset swatch in tokens.json. */
export const ACCENT_HUES: Record<AccentPreset, number> = {
  amber: 62,
  lime: 128,
  mint: 168,
  sky: 232,
  indigo: 275,
  violet: 302,
  rose: 360,
  graphite: 56,
};

export const ACCENT_PRESETS: readonly AccentPreset[] = [
  "amber",
  "lime",
  "mint",
  "sky",
  "indigo",
  "violet",
  "rose",
  "graphite",
];

export const ACCENT_LABELS: Record<AccentPreset, string> = {
  amber: "Amber",
  lime: "Lime",
  mint: "Mint",
  sky: "Sky",
  indigo: "Indigo",
  violet: "Violet",
  rose: "Rose",
  graphite: "Graphite",
};

export interface Oklch {
  l: number;
  c: number;
  h: number;
}

const fmt = (n: number, digits = 3) => Number(n.toFixed(digits)).toString();
export const oklchString = ({ l, c, h }: Oklch) => `oklch(${fmt(l)} ${fmt(c)} ${fmt(h, 1)})`;

/** The OKLCH values for one preset in one theme. */
export function accentColors(preset: AccentPreset, theme: ThemeName): Record<AccentToken, Oklch> {
  const h = ACCENT_HUES[preset];
  if (preset === "graphite") {
    // Neutral: a dark accent in light, a light accent in dark; text on it is the canvas colour.
    return theme === "light"
      ? {
          accent: { l: 0.3, c: 0.006, h },
          "accent-hover": { l: 0.26, c: 0.006, h },
          "accent-text": { l: 0.33, c: 0.006, h },
          "accent-soft": { l: 0.93, c: 0.004, h },
          "on-accent": { l: 0.99, c: 0.002, h },
        }
      : {
          accent: { l: 0.88, c: 0.004, h },
          "accent-hover": { l: 0.92, c: 0.004, h },
          "accent-text": { l: 0.86, c: 0.004, h },
          "accent-soft": { l: 0.32, c: 0.006, h },
          "on-accent": { l: 0.22, c: 0.004, h },
        };
  }
  return theme === "light"
    ? {
        accent: { l: 0.76, c: 0.165, h },
        "accent-hover": { l: 0.72, c: 0.165, h },
        "accent-text": { l: 0.52, c: 0.14, h },
        "accent-soft": { l: 0.955, c: 0.035, h },
        "on-accent": { l: 0.24, c: 0.04, h },
      }
    : {
        accent: { l: 0.77, c: 0.155, h },
        "accent-hover": { l: 0.81, c: 0.155, h },
        "accent-text": { l: 0.81, c: 0.13, h },
        "accent-soft": { l: 0.295, c: 0.055, h },
        "on-accent": { l: 0.22, c: 0.04, h },
      };
}

/** CSS values for one preset in one theme. Graphite's `on-accent` points at `bg-canvas`. */
export function accentTokens(preset: AccentPreset, theme: ThemeName): AccentTokens {
  const colors = accentColors(preset, theme);
  return {
    accent: oklchString(colors.accent),
    "accent-hover": oklchString(colors["accent-hover"]),
    "accent-text": oklchString(colors["accent-text"]),
    "accent-soft": oklchString(colors["accent-soft"]),
    "on-accent": preset === "graphite" ? "var(--bg-canvas)" : oklchString(colors["on-accent"]),
  };
}

/**
 * The stylesheet that overrides the generated tokens for a preset, or "" for the default
 * (amber keeps the hand-tuned values in tokens.css). Covers explicit and system themes.
 */
export function accentStylesheet(preset: AccentPreset): string {
  if (preset === "amber") return "";
  const block = (theme: ThemeName) =>
    Object.entries(accentTokens(preset, theme))
      .map(([name, value]) => `--${name}: ${value};`)
      .join(" ");
  const light = block("light");
  const dark = block("dark");
  return [
    `:root, [data-theme="light"] { ${light} }`,
    `[data-theme="dark"] { ${dark} }`,
    `@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { ${dark} } }`,
  ].join("\n");
}

// ───────── Colour maths (OKLCH → sRGB), used to verify contrast ─────────

/** OKLCH to gamma-encoded sRGB in 0..1, clamped to the gamut. */
export function oklchToSrgb({ l, c, h }: Oklch): [number, number, number] {
  const hr = (h * Math.PI) / 180;
  const a = c * Math.cos(hr);
  const b = c * Math.sin(hr);
  const l_ = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m_ = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s_ = (l - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const lin = [
    4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
    -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
    -0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_,
  ];
  const encode = (x: number) => {
    const v = Math.min(1, Math.max(0, x));
    return v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
  };
  return [encode(lin[0] ?? 0), encode(lin[1] ?? 0), encode(lin[2] ?? 0)];
}

export function hexToSrgb(hex: string): [number, number, number] {
  const n = Number.parseInt(hex.replace("#", "").slice(0, 6), 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

export function relativeLuminance([r, g, b]: [number, number, number]): number {
  const lin = (x: number) => (x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

export function contrastRatio(a: [number, number, number], b: [number, number, number]): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}
