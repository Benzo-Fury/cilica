/**
 * The setup window's background: a dithered pixel ridgeline with drifting clouds. Dawn in the
 * light theme, night with stars in the dark one. Drawn in code at a third of the window's size and
 * scaled up without smoothing, so it ships no image and costs nothing in the installer.
 *
 * Every colour comes from the theme tokens (`bg-canvas`, `ink`, `home-accent`), mixed per layer, so
 * the scene repaints itself when the theme changes. Reduce motion keeps it still.
 */
import { useEffect, useRef } from "react";

type Rgb = [number, number, number];

/** Pixel size: each scene pixel covers this many CSS pixels. */
const SCALE = 3;
/** Repaint at most this often while drifting; the motion is slow. */
const FRAME_MS = 50;
const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5].map((v) => (v + 0.5) / 16);
const CLOUDS = [
  { x: 0.12, y: 0.64, r: 0.11 },
  { x: 0.58, y: 0.6, r: 0.08 },
  { x: 0.95, y: 0.67, r: 0.1 },
] as const;
const RIDGES = 5;

/** Parse a computed colour: `#rgb`, `#rrggbb(aa)` or `rgb(a)(…)`. */
export function parseColor(value: string): Rgb | null {
  const v = value.trim();
  if (v.startsWith("#")) {
    const hex =
      v.length === 4
        ? v
            .slice(1)
            .split("")
            .map((c) => c + c)
            .join("")
        : v.slice(1, 7);
    if (hex.length !== 6) return null;
    const n = Number.parseInt(hex, 16);
    return Number.isNaN(n) ? null : [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }
  const m = v.match(/rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/);
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

const mix = (a: Rgb, b: Rgb, t: number): Rgb => [
  Math.round(a[0] + (b[0] - a[0]) * t),
  Math.round(a[1] + (b[1] - a[1]) * t),
  Math.round(a[2] + (b[2] - a[2]) * t),
];

const luminance = ([r, g, b]: Rgb) => (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;

export interface ScenePalette {
  dark: boolean;
  sky: Rgb[];
  sun: Rgb;
  sunCore: Rgb;
  ridges: Rgb[];
  ridgeEdges: Rgb[];
  cloud: Rgb;
  cloudShade: Rgb;
  star: Rgb | null;
}

/** Derive the scene from three tokens. Far ridges sit in the haze; near ones carry the most ink. */
export function scenePalette(canvas: Rgb, ink: Rgb, warm: Rgb): ScenePalette {
  const dark = luminance(canvas) < 0.5;
  const sky = [0, 0.05, 0.12, 0.2, 0.28].map((t) => mix(canvas, warm, dark ? t * 0.5 : t));
  const ridges = Array.from({ length: RIDGES }, (_, i) => {
    const depth = i / (RIDGES - 1);
    const t = dark ? 0.22 - depth * 0.1 : 0.1 + depth * 0.26;
    return mix(mix(canvas, warm, dark ? 0.08 : 0.16), ink, t);
  });
  const ridgeEdges = ridges.map((r) => mix(r, dark ? ink : canvas, dark ? 0.07 : 0.14));
  return {
    dark,
    sky,
    sun: dark ? mix(ink, warm, 0.25) : mix(canvas, warm, 0.35),
    sunCore: dark ? mix(ink, warm, 0.1) : mix(canvas, warm, 0.12),
    ridges,
    ridgeEdges,
    cloud: dark ? mix(canvas, ink, 0.12) : mix(canvas, ink, 0.01),
    cloudShade: dark ? mix(canvas, ink, 0.08) : mix(mix(canvas, warm, 0.08), ink, 0.06),
    star: dark ? mix(ink, warm, 0.2) : null,
  };
}

const rnd = (i: number, s: number) => {
  const x = Math.sin(i * 127.1 + s * 311.7) * 43758.5453;
  return x - Math.floor(x);
};
const noise = (x: number, s: number) => {
  const i = Math.floor(x);
  const f = x - i;
  const u = f * f * (3 - 2 * f);
  return rnd(i, s) * (1 - u) + rnd(i + 1, s) * u;
};
const fbm = (x: number, s: number) =>
  noise(x, s) * 0.6 + noise(x * 2.1, s + 3) * 0.27 + noise(x * 4.3, s + 7) * 0.13;

/** Paint one frame into `img` (width × height scene pixels) at time `t` in ms. */
export function paintScene(img: ImageData, p: ScenePalette, t: number) {
  const { width: W, height: H, data } = img;
  const drift = t * 0.000012;
  const tops = p.ridges.map((_, L) => {
    const base = H * (0.74 + L * 0.05);
    const amp = H * (0.1 - L * 0.01);
    const freq = 2.2 + L * 0.9;
    const parallax = 0.15 + L * 0.35;
    return Array.from(
      { length: W },
      (_, x) => base - amp * fbm((x / W) * freq + L * 9.3 + drift * parallax * 40, L + 1),
    );
  });
  const sunX = W * 0.74;
  const sunY = H * 0.72;
  const sunR = W * 0.06;
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const b = BAYER[(y & 3) * 4 + (x & 3)] ?? 0.5;
      let c: Rgb;
      let layer = -1;
      for (let k = p.ridges.length - 1; k >= 0; k--) {
        if (y >= (tops[k]?.[x] ?? H)) {
          layer = k;
          break;
        }
      }
      if (layer >= 0) {
        const depth = y - (tops[layer]?.[x] ?? H);
        const edge =
          depth < 2 + (b > 0.5 ? 1 : 0) || (layer >= 2 && rnd(x * 7 + y * 13, layer) > 0.93);
        c = (edge ? p.ridgeEdges[layer] : p.ridges[layer]) as Rgb;
      } else {
        const v = (y / (H * 0.86)) * (p.sky.length - 1);
        const i0 = Math.min(p.sky.length - 1, Math.floor(v));
        c = p.sky[Math.min(p.sky.length - 1, v - i0 > b ? i0 + 1 : i0)] as Rgb;
        const dist = Math.hypot(x - sunX, y - sunY);
        if (dist < sunR) c = dist < sunR * 0.72 ? p.sunCore : p.sun;
        else if (dist < sunR * 2.4 && (1 - (dist - sunR) / (sunR * 1.4)) * 0.55 > b) c = p.sun;
        if (
          p.star &&
          y < H * 0.72 &&
          rnd(x * 31 + y * 17, 2) > 0.994 &&
          (rnd(x + y, 5) + t * 0.0004) % 1 > 0.25
        )
          c = p.star;
        for (const cl of CLOUDS) {
          const cx = (((cl.x + drift * 26) % 1.3) - 0.15) * W;
          const cy = cl.y * H;
          const r = cl.r * W;
          const ex = (x - cx) / (r * 2.2);
          const ey = (y - cy) / r;
          const f =
            Math.max(0, 1 - (ex * ex + ey * ey)) +
            Math.max(
              0,
              1 - (((x - cx - r * 0.9) / (r * 1.3)) ** 2 + ((y - cy + r * 0.35) / (r * 0.8)) ** 2),
            ) +
            Math.max(
              0,
              1 - (((x - cx + r) / (r * 1.1)) ** 2 + ((y - cy + r * 0.15) / (r * 0.7)) ** 2),
            );
          if (f > 0.35 + b * 0.25) c = ey > 0.25 ? p.cloudShade : p.cloud;
        }
      }
      const o = (y * W + x) * 4;
      data[o] = c[0];
      data[o + 1] = c[1];
      data[o + 2] = c[2];
      data[o + 3] = 255;
    }
  }
}

function readPalette(): ScenePalette | null {
  const style = getComputedStyle(document.documentElement);
  const canvas = parseColor(style.getPropertyValue("--bg-canvas"));
  const ink = parseColor(style.getPropertyValue("--ink"));
  const warm = parseColor(style.getPropertyValue("--home-accent"));
  return canvas && ink && warm ? scenePalette(canvas, ink, warm) : null;
}

export function SceneArt({ still = false }: { still?: boolean }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const cv = ref.current;
    const ctx = cv?.getContext("2d");
    if (!cv || !ctx) return;
    const reduced = still || matchMedia("(prefers-reduced-motion: reduce)").matches;
    const t0 = performance.now();
    let palette = readPalette();
    let raf = 0;
    let last = 0;
    let img: ImageData | null = null;

    const size = () => {
      const w = Math.max(1, Math.round(cv.clientWidth / SCALE));
      const h = Math.max(1, Math.round(cv.clientHeight / SCALE));
      if (cv.width !== w || cv.height !== h || !img) {
        cv.width = w;
        cv.height = h;
        img = ctx.createImageData(w, h);
      }
    };
    const draw = (now: number) => {
      if (!palette || !img) return;
      paintScene(img, palette, now - t0);
      ctx.putImageData(img, 0, 0);
    };
    const frame = (now: number) => {
      raf = requestAnimationFrame(frame);
      if (now - last < FRAME_MS) return;
      last = now;
      draw(now);
    };
    const repaint = () => {
      palette = readPalette();
      size();
      draw(performance.now());
    };

    repaint();
    if (!reduced) raf = requestAnimationFrame(frame);
    const resize = new ResizeObserver(repaint);
    resize.observe(cv);
    // The theme lives on <html> (`data-theme`), or follows the system when absent.
    const themeAttr = new MutationObserver(repaint);
    themeAttr.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    const scheme = matchMedia("(prefers-color-scheme: dark)");
    scheme.addEventListener("change", repaint);
    const visibility = () => {
      cancelAnimationFrame(raf);
      if (!document.hidden && !reduced) raf = requestAnimationFrame(frame);
    };
    document.addEventListener("visibilitychange", visibility);
    return () => {
      cancelAnimationFrame(raf);
      resize.disconnect();
      themeAttr.disconnect();
      scheme.removeEventListener("change", repaint);
      document.removeEventListener("visibilitychange", visibility);
    };
  }, [still]);
  return <canvas ref={ref} />;
}
