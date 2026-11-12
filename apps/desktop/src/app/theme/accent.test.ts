import {
  ACCENT_PRESETS,
  accentColors,
  accentStylesheet,
  accentTokens,
  contrastRatio,
  hexToSrgb,
  oklchToSrgb,
} from "./accent";

const CANVAS = { light: hexToSrgb("#fdfbfa"), dark: hexToSrgb("#1b1917") };

describe("accent presets", () => {
  it("keeps the README's lightness and chroma for hued presets", () => {
    const light = accentColors("sky", "light");
    expect(light.accent).toEqual({ l: 0.76, c: 0.165, h: 232 });
    expect(light["accent-text"]).toEqual({ l: 0.52, c: 0.14, h: 232 });
    expect(light["accent-soft"]).toEqual({ l: 0.955, c: 0.035, h: 232 });
    const dark = accentColors("sky", "dark");
    expect(dark.accent).toEqual({ l: 0.77, c: 0.155, h: 232 });
    expect(dark["accent-text"]).toEqual({ l: 0.81, c: 0.13, h: 232 });
    expect(dark["accent-soft"]).toEqual({ l: 0.295, c: 0.055, h: 232 });
  });

  it("makes hover 0.04 darker in light and 0.04 lighter in dark", () => {
    for (const preset of ACCENT_PRESETS) {
      const light = accentColors(preset, "light");
      const dark = accentColors(preset, "dark");
      expect(light["accent-hover"].l).toBeCloseTo(light.accent.l - 0.04, 5);
      expect(dark["accent-hover"].l).toBeCloseTo(dark.accent.l + 0.04, 5);
    }
  });

  it("formats tokens as oklch() strings", () => {
    expect(accentTokens("violet", "light").accent).toBe("oklch(0.76 0.165 302)");
    expect(accentTokens("graphite", "light")["on-accent"]).toBe("var(--bg-canvas)");
  });

  it("keeps text on the accent and accent text on the canvas readable (4.5:1)", () => {
    for (const preset of ACCENT_PRESETS) {
      for (const theme of ["light", "dark"] as const) {
        const c = accentColors(preset, theme);
        const accent = oklchToSrgb(c.accent);
        expect(contrastRatio(accent, oklchToSrgb(c["on-accent"]))).toBeGreaterThanOrEqual(4.5);
        expect(contrastRatio(oklchToSrgb(c["accent-text"]), CANVAS[theme])).toBeGreaterThanOrEqual(
          4.5,
        );
      }
    }
  });

  it("leaves amber to the hand-tuned tokens and covers every theme mode otherwise", () => {
    expect(accentStylesheet("amber")).toBe("");
    const css = accentStylesheet("mint");
    expect(css).toContain('[data-theme="dark"]');
    expect(css).toContain("prefers-color-scheme: dark");
    expect(css).toContain("--accent-soft: oklch(0.955 0.035 168)");
  });

  it("round-trips the default amber close to its hex token", () => {
    const [r, g, b] = oklchToSrgb({ l: 0.76, c: 0.165, h: 62 });
    const [hr, hg, hb] = hexToSrgb("#fa9524");
    expect(Math.abs(r - hr)).toBeLessThan(0.03);
    expect(Math.abs(g - hg)).toBeLessThan(0.03);
    expect(Math.abs(b - hb)).toBeLessThan(0.03);
  });
});
