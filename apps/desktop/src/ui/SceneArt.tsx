/**
 * Dithered pixel scenes: the setup window's background and the art behind the scene themes. The
 * ridgeline has drifting clouds, dawn in a light theme and night with stars in a dark one; the pine
 * forest and the desert are still. Drawn in code at a third of the window's size and scaled up
 * without smoothing, so they ship no image and cost nothing in the installer.
 *
 * Every colour comes from the theme tokens (`bg-canvas`, `ink`, `scene-tint`), mixed per layer, so
 * a scene repaints itself when the theme changes. Reduce motion keeps it still.
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

/** The three colours a scene is mixed from: the page, the text and the theme's tint. */
export interface SceneTones {
  canvas: Rgb;
  ink: Rgb;
  tint: Rgb;
}

export type SceneKind = "ridgeline" | "pines" | "mesa";

const at = (a: ArrayLike<number>, i: number) => a[i] ?? 0;
const bayer = (x: number, y: number) => BAYER[(y & 3) * 4 + (x & 3)] ?? 0.5;

/** One of `bands`, dithered between neighbours as `v` runs from 0 to the last index. */
function band(bands: Rgb[], v: number, b: number): Rgb {
  const last = bands.length - 1;
  const i0 = Math.min(last, Math.max(0, Math.floor(v)));
  return bands[Math.min(last, v - i0 > b ? i0 + 1 : i0)] as Rgb;
}

/** A sun: a core, a rim and a dithered halo. `null` outside it. */
function disc(
  x: number,
  y: number,
  b: number,
  cx: number,
  cy: number,
  r: number,
  core: Rgb,
  glow: Rgb,
): Rgb | null {
  const dist = Math.hypot(x - cx, y - cy);
  if (dist < r) return dist < r * 0.72 ? core : glow;
  if (dist < r * 2.4 && (1 - (dist - r) / (r * 1.4)) * 0.55 > b) return glow;
  return null;
}

function put(data: Uint8ClampedArray, o: number, c: Rgb) {
  data[o] = c[0];
  data[o + 1] = c[1];
  data[o + 2] = c[2];
  data[o + 3] = 255;
}

/** A pine forest in morning mist: four ranks of trees, the far ones lost in haze. Light themes. */
export function paintPines(img: ImageData, { canvas, ink, tint }: SceneTones) {
  const { width: W, height: H, data } = img;
  const unit = H / 267;
  const ranks = 4;
  const colours = Array.from({ length: ranks }, (_, k) =>
    mix(mix(canvas, tint, 0.22 + 0.1 * k), ink, 0.06 + 0.09 * k),
  );
  const mist = mix(canvas, tint, 0.1);
  const hazed = colours.map((c) => mix(c, mist, 0.55));
  const sky = [0, 0.05, 0.12, 0.2, 0.28].map((t) => mix(canvas, tint, t));
  const ridges: Float32Array[] = [];
  const trees: Uint8Array[] = [];
  for (let k = 0; k < ranks; k++) {
    const base = H * (0.69 + k * 0.08);
    const amp = H * (0.06 - k * 0.006);
    const ridge = new Float32Array(W);
    for (let x = 0; x < W; x++) {
      ridge[x] = base - amp * fbm((x / W) * (2 + k * 0.8) + k * 5.1, k + 11);
    }
    const mask = new Uint8Array(W * H);
    let x = -8 * unit;
    for (let i = 0; x < W + 8 * unit; i++) {
      const h = (9 + k * 8) * unit * (0.55 + 0.9 * rnd(i, k + 20));
      const w = Math.max(2, h * 0.34);
      const tx = Math.floor(x);
      const tip = Math.floor(at(ridge, Math.min(W - 1, Math.max(0, tx))) - h) + 1;
      const tier = h / 3;
      for (let p = 0; p < h + 2; p++) {
        const n = Math.min(2, Math.floor(p / tier));
        const f = Math.min(1, (p - n * tier) / tier);
        const half = Math.floor(w * (0.25 + 0.75 * f) * ((n + 1.5) / 3.5));
        const yy = tip + p;
        if (yy < 0 || yy >= H) continue;
        for (let dx = -half; dx <= half; dx++) {
          if (tx + dx >= 0 && tx + dx < W) mask[yy * W + tx + dx] = 1;
        }
      }
      x += w * 1.15 + rnd(i, k + 30) * (7 - k) * unit;
    }
    ridges.push(ridge);
    trees.push(mask);
  }
  const sunCore = mix(canvas, tint, 0.04);
  const sunGlow = mix(canvas, tint, 0.3);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const b = bayer(x, y);
      let c: Rgb | null = null;
      for (let k = ranks - 1; k >= 0; k--) {
        const top = at(ridges[k] as Float32Array, x);
        if (y >= top || at(trees[k] as Uint8Array, y * W + x)) {
          const depth = y - top;
          const lost = k < ranks - 1 && depth > 0 && depth / (H * 0.07) > b + 0.15;
          c = (lost ? hazed[k] : colours[k]) as Rgb;
          break;
        }
      }
      c ??=
        disc(x, y, b, W * 0.3, H * 0.6, W * 0.05, sunCore, sunGlow) ??
        band(sky, (y / (H * 0.86)) * (sky.length - 1), b);
      put(data, (y * W + x) * 4, c);
    }
  }
}

/** Desert buttes at sunset: two ranks of flat-topped rock, dunes and a few cacti. Light themes. */
export function paintMesa(img: ImageData, { canvas, ink, tint }: SceneTones) {
  const { width: W, height: H, data } = img;
  const unit = H / 267;
  const colours = Array.from({ length: 4 }, (_, k) =>
    mix(mix(canvas, tint, 0.38 + 0.1 * k), ink, 0.05 + 0.1 * k),
  );
  const edges = colours.map((c) => mix(c, canvas, 0.22));
  const cactusColour = mix(colours[3] as Rgb, ink, 0.4);
  const sky = [0, 0.06, 0.14, 0.24, 0.36].map((t) => mix(canvas, tint, t));
  const plateau = (n: number) =>
    Math.min(1, Math.max(0, (n - 0.42) * 9)) * 0.6 +
    Math.min(1, Math.max(0, (n - 0.66) * 12)) * 0.4;
  const tops = [
    (x: number) => H * 0.83 - H * 0.2 * plateau(noise((x / W) * 3.4 + 0.7, 31)),
    (x: number) => H * 0.88 - H * 0.15 * plateau(noise((x / W) * 2.3 + 4.1, 41)),
    (x: number) => H * 0.93 - H * 0.05 * fbm((x / W) * 2.6 + 2.0, 51),
    (x: number) => H * 0.99 - H * 0.04 * fbm((x / W) * 3.4 + 8.0, 61),
  ].map((top) => Float32Array.from({ length: W }, (_, x) => top(x)));
  const strata = Math.max(3, Math.round(6 * unit));
  const cacti = new Uint8Array(W * H);
  const mark = (x: number, y: number) => {
    if (x >= 0 && x < W && y >= 0 && y < H) cacti[y * W + x] = 1;
  };
  const girth = unit > 0.6 ? 1 : 0;
  for (const [fx, scale] of [
    [0.09, 1],
    [0.36, 0.7],
    [0.9, 1.2],
  ] as const) {
    const cx = Math.floor(W * fx);
    const base = Math.floor(at(tops[3] as Float32Array, cx)) + 2;
    const tall = Math.round(18 * scale * unit);
    const reach = Math.round(5 * scale * unit) + 1 + girth;
    for (let y = base - tall; y < base; y++) {
      for (let dx = -girth; dx <= girth; dx++) mark(cx + dx, y);
    }
    const left = base - Math.round(tall * 0.45);
    const right = base - Math.round(tall * 0.3);
    for (let dx = 1; dx <= reach; dx++) {
      mark(cx - dx, left);
      mark(cx + dx, right);
    }
    for (let y = base - Math.round(tall * 0.75); y <= left; y++) {
      for (let dx = 0; dx <= girth; dx++) mark(cx - reach + dx, y);
    }
    for (let y = base - Math.round(tall * 0.62); y <= right; y++) {
      for (let dx = 0; dx <= girth; dx++) mark(cx + reach - dx, y);
    }
  }
  const sunCore = mix(canvas, tint, 0.2);
  const sunGlow = mix(canvas, tint, 0.55);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const b = bayer(x, y);
      let c: Rgb | null = at(cacti, y * W + x) ? cactusColour : null;
      for (let k = 3; k >= 0 && !c; k--) {
        const depth = y - at(tops[k] as Float32Array, x);
        if (depth < 0) continue;
        const lit =
          depth < 2 + (b > 0.5 ? 1 : 0) ||
          (k < 2
            ? Math.floor(depth) % strata === strata - 1 && b > 0.25
            : rnd(x * 7 + y * 13, k) > 0.94);
        c = (lit ? edges[k] : colours[k]) as Rgb;
      }
      c ??=
        disc(x, y, b, W * 0.6, H * 0.75, W * 0.09, sunCore, sunGlow) ??
        band(sky, (y / (H * 0.86)) * (sky.length - 1), b);
      put(data, (y * W + x) * 4, c);
    }
  }
}

const PAINTERS: Record<SceneKind, (img: ImageData, tones: SceneTones, t: number) => void> = {
  ridgeline: (img, { canvas, ink, tint }, t) => paintScene(img, scenePalette(canvas, ink, tint), t),
  pines: paintPines,
  mesa: paintMesa,
};

/** The tokens in effect on `el`, so a preview inside a themed element paints that theme. */
function readTones(el: Element): SceneTones | null {
  const style = getComputedStyle(el);
  const canvas = parseColor(style.getPropertyValue("--bg-canvas"));
  const ink = parseColor(style.getPropertyValue("--ink"));
  const tint =
    parseColor(style.getPropertyValue("--scene-tint")) ??
    parseColor(style.getPropertyValue("--home-accent"));
  return canvas && ink && tint ? { canvas, ink, tint } : null;
}

export interface SceneArtProps {
  /** Which scene to paint. Only the ridgeline drifts; the others are always still. */
  scene?: SceneKind;
  /** Paint once and on resize or theme change, never on a timer. */
  still?: boolean;
  /** CSS pixels covered by each scene pixel. Small previews use 1. */
  pixel?: number;
}

export function SceneArt({ scene = "ridgeline", still = false, pixel = SCALE }: SceneArtProps) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const cv = ref.current;
    const ctx = cv?.getContext("2d");
    if (!cv || !ctx) return;
    const paint = PAINTERS[scene];
    const reduced =
      still || scene !== "ridgeline" || matchMedia("(prefers-reduced-motion: reduce)").matches;
    const t0 = performance.now();
    let tones = readTones(cv);
    let raf = 0;
    let last = 0;
    let img: ImageData | null = null;

    const size = () => {
      const w = Math.max(1, Math.round(cv.clientWidth / pixel));
      const h = Math.max(1, Math.round(cv.clientHeight / pixel));
      if (cv.width !== w || cv.height !== h || !img) {
        cv.width = w;
        cv.height = h;
        img = ctx.createImageData(w, h);
      }
    };
    const draw = (now: number) => {
      if (!tones || !img) return;
      paint(img, tones, now - t0);
      ctx.putImageData(img, 0, 0);
    };
    const frame = (now: number) => {
      raf = requestAnimationFrame(frame);
      if (now - last < FRAME_MS) return;
      last = now;
      draw(now);
    };
    const repaint = () => {
      tones = readTones(cv);
      size();
      draw(performance.now());
    };

    repaint();
    if (!reduced) raf = requestAnimationFrame(frame);
    const resize = new ResizeObserver(repaint);
    resize.observe(cv);
    // The theme lives on <html> (`data-theme` and `data-look`), or follows the system when absent.
    const themeAttr = new MutationObserver(repaint);
    themeAttr.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme", "data-look"],
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
  }, [scene, still, pixel]);
  return <canvas ref={ref} />;
}

/** A scene theme's art behind the whole window: the still scene under a veil that keeps the top clear. */
export function SceneBackdrop({ scene }: { scene: SceneKind }) {
  return (
    <div className="nt-scene" aria-hidden>
      <SceneArt scene={scene} still />
      <div className="nt-scene-veil" />
    </div>
  );
}
