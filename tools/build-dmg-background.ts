/**
 * Generate apps/desktop/src-tauri/dmg/background.tiff, the picture behind the macOS disk image
 * window ("Drag Margin into Applications"). It is the setup window's pixel ridgeline in the dark
 * theme, painted by the app's own SceneArt, with the title and arrow set over it by headless
 * Chrome. A 1x and a 2x render are joined into one TIFF so Retina screens get the sharp one.
 *
 * Finder draws the two icons and their names itself, at bundle.macOS.dmg.appPosition and
 * applicationFolderPosition in tauri.conf.json; the arrow here sits between those points.
 *
 * Run: bun run dmg-background   (needs Google Chrome, or CHROME=/path/to/chrome; macOS only)
 */
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { deflateSync } from "node:zlib";
import { paintScene, parseColor, scenePalette } from "../apps/desktop/src/ui/SceneArt";

type Rgb = [number, number, number];

// ---------- Layout ---------- //

/**
 * The window's content size in points. bundle.macOS.dmg.windowSize is 28pt taller: Tauri's size
 * includes Finder's title bar.
 */
const WIDTH = 660;
const HEIGHT = 400;
/**
 * Finder draws the icon names itself, in black whatever the Mac's appearance, and has no setting
 * for their colour. On the night art they need a light plate. Each plate is centred under its
 * icon (x from appPosition and applicationFolderPosition) where Finder puts the 16pt name, and is
 * sized to the name with room to spare.
 */
const PLATES = [
  { x: 180, width: 84 },
  { x: 480, width: 120 },
] as const;
/** The icons' y in tauri.conf.json, and how far below it Finder centres a name. */
const ICON_Y = 160;
const NAME_OFFSET = 86;
const PLATE_HEIGHT = 26;
/** Each scene pixel covers this many points, as in the setup window. */
const SCENE_SCALE = 3;
/**
 * The scene is painted this tall and only its bottom HEIGHT points kept, so the ridges keep the
 * relief they have in the portrait setup window instead of flattening in a wide one.
 */
const SCENE_HEIGHT = 600;

const ROOT = resolve(import.meta.dir, "..");
const OUT = join(ROOT, "apps/desktop/src-tauri/dmg/background.tiff");
const NEWSREADER = join(ROOT, "apps/desktop/node_modules/@fontsource-variable/newsreader/files");
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

// ---------- Theme ---------- //

/** The dark theme's colours from tokens.json, the only ones the background uses. */
class DarkTheme {
  readonly canvas: Rgb;
  readonly ink: Rgb;
  readonly ink3: Rgb;
  readonly warm: Rgb;

  /** Read the dark values of bg-canvas, ink, ink-3 and home-accent. */
  constructor(tokensPath: string) {
    const tokens: { color: { tokens: { name: string; value: { dark: string } }[] } } = JSON.parse(
      readFileSync(tokensPath, "utf8"),
    );
    const dark = (name: string): Rgb => {
      const value = tokens.color.tokens.find((t) => t.name === name)?.value.dark;
      const rgb = value ? parseColor(value) : null;
      if (!rgb) throw new Error(`tokens.json has no dark value for ${name}`);
      return rgb;
    };
    this.canvas = dark("bg-canvas");
    this.ink = dark("ink");
    this.ink3 = dark("ink-3");
    this.warm = dark("home-accent");
  }

  /** A colour as `#rrggbb` for the page's CSS. */
  static hex(rgb: Rgb): string {
    return `#${rgb.map((c) => c.toString(16).padStart(2, "0")).join("")}`;
  }
}

// ---------- Background ---------- //

/** Paints the art, sets the type over it and writes the two-resolution TIFF. */
class DmgBackground {
  /** Scratch folder for the art, the page and the renders; removed when done. */
  private readonly work = mkdtempSync(join(tmpdir(), "margin-dmg-"));

  constructor(private readonly theme: DarkTheme) {}

  /** Build the TIFF at OUT, cleaning up the scratch folder whatever happens. */
  build() {
    try {
      // Paint the art once at 2x; the 1x render downsamples it pixel for pixel
      const art = this.paintArt(2);
      const page = join(this.work, "background.html");
      writeFileSync(page, this.page(art));

      // Render both resolutions and join them
      const shots = [1, 2].map((scale) => this.screenshot(page, scale));
      mkdirSync(dirname(OUT), { recursive: true });
      this.run("tiffutil", ["-cathidpicheck", ...shots, "-out", OUT]);
      console.log(`Wrote ${OUT.slice(ROOT.length + 1)}`);
    } finally {
      rmSync(this.work, { recursive: true, force: true });
    }
  }

  /**
   * The ridgeline at `scale` device pixels per point, with the setup window's veil baked in:
   * solid bg-canvas over the top 40% so the title never sits on the art, clear by 86%.
   */
  private paintArt(scale: number): Buffer {
    const { canvas } = this.theme;

    // Paint the scene in scene pixels, keeping its bottom HEIGHT points
    const sw = WIDTH / SCENE_SCALE;
    const sh = SCENE_HEIGHT / SCENE_SCALE;
    const crop = Math.round((SCENE_HEIGHT - HEIGHT) / SCENE_SCALE);
    const img = { width: sw, height: sh, data: new Uint8ClampedArray(sw * sh * 4) };
    paintScene(img as unknown as ImageData, scenePalette(canvas, this.theme.ink, this.theme.warm), 9000);

    // Scale up without smoothing and lay the veil over it
    const w = WIDTH * scale;
    const h = HEIGHT * scale;
    const rgb = new Uint8Array(w * h * 3);
    for (let y = 0; y < h; y++) {
      const veil = DmgBackground.veil(y / scale / HEIGHT);
      const sy = Math.min(sh - 1, Math.floor(y / scale / SCENE_SCALE) + crop);
      for (let x = 0; x < w; x++) {
        const sx = Math.min(sw - 1, Math.floor(x / scale / SCENE_SCALE));
        const from = (sy * sw + sx) * 4;
        for (let c = 0; c < 3; c++) {
          rgb[(y * w + x) * 3 + c] = Math.round(
            (img.data[from + c] ?? 0) * (1 - veil) + (canvas[c] ?? 0) * veil,
          );
        }
      }
    }
    return Png.encode(w, h, rgb);
  }

  /** How much bg-canvas covers the art at `t`, a fraction of the height from the top. */
  private static veil(t: number): number {
    if (t <= 0.4) return 1;
    if (t <= 0.58) return 1 - ((t - 0.4) / 0.18) * 0.22;
    if (t <= 0.86) return 0.78 * (1 - (t - 0.58) / 0.28);
    return 0;
  }

  /**
   * The page Chrome renders: the art, the title in Newsreader, the arrow between the icons and
   * the plates under their names.
   */
  private page(art: Buffer): string {
    const { hex } = DarkTheme;
    const font = (file: string) => `url("file://${join(NEWSREADER, file)}") format("woff2")`;
    const plateTop = ICON_Y + NAME_OFFSET - PLATE_HEIGHT / 2;
    const plates = PLATES.map(
      (p) =>
        `<div class="plate" style="left: ${p.x - p.width / 2}px; top: ${plateTop}px; width: ${p.width}px"></div>`,
    ).join("\n");
    return `<!doctype html>
<meta charset="utf-8">
<style>
@font-face { font-family: Newsreader; font-style: normal; font-weight: 200 800; src: ${font("newsreader-latin-opsz-normal.woff2")}; }
@font-face { font-family: Newsreader; font-style: italic; font-weight: 200 800; src: ${font("newsreader-latin-opsz-italic.woff2")}; }
html, body { margin: 0; width: ${WIDTH}px; height: ${HEIGHT}px; overflow: hidden; background: ${hex(this.theme.canvas)}; }
img { position: absolute; inset: 0; width: ${WIDTH}px; height: ${HEIGHT}px; image-rendering: pixelated; }
h1 { position: absolute; top: 40px; left: 0; right: 0; margin: 0; text-align: center; color: ${hex(this.theme.ink)};
  font: 500 30px/1.05 Newsreader, serif; font-optical-sizing: auto; letter-spacing: -0.015em; }
svg { position: absolute; left: 270px; top: 136px; color: ${hex(this.theme.ink3)}; }
.plate { position: absolute; height: ${PLATE_HEIGHT}px; border-radius: ${PLATE_HEIGHT / 2}px;
  background: ${hex(this.theme.ink)}; box-shadow: 0 1px 3px rgb(0 0 0 / 0.35); }
</style>
<img src="data:image/png;base64,${art.toString("base64")}" alt="">
<h1>Drag <em>Margin</em> into Applications</h1>
${plates}
<svg width="120" height="40" viewBox="0 0 120 40" fill="none">
  <path d="M4 26 C 34 8, 80 8, 112 24" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-dasharray="1 7"/>
  <path d="M100 15 L 113 24.5 L 98 29" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"/>
</svg>`;
  }

  /** Render the page with headless Chrome at `scale` and return the PNG's path. */
  private screenshot(page: string, scale: number): string {
    if (!existsSync(CHROME)) {
      throw new Error(`Chrome was not found at ${CHROME}. Install it or set CHROME to its binary.`);
    }
    const out = join(this.work, `background@${scale}x.png`);
    // The virtual time budget lets the fonts load before the shot
    this.run(CHROME, [
      "--headless=new",
      "--disable-gpu",
      "--hide-scrollbars",
      "--allow-file-access-from-files",
      "--virtual-time-budget=3000",
      `--force-device-scale-factor=${scale}`,
      `--window-size=${WIDTH},${HEIGHT}`,
      `--screenshot=${out}`,
      `file://${page}`,
    ]);
    if (!existsSync(out)) throw new Error(`Chrome did not write ${out}`);
    return out;
  }

  /** Run a tool and fail with its output when it exits non-zero. */
  private run(cmd: string, args: string[]) {
    const result = spawnSync(cmd, args, { encoding: "utf8" });
    if (result.status !== 0) {
      throw new Error(`${cmd} failed: ${result.stderr || result.stdout || result.error?.message}`);
    }
  }
}

// ---------- PNG ---------- //

/** A minimal RGB PNG encoder, so the art needs no image library. */
class Png {
  private static readonly crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });

  /** Encode `rgb` (3 bytes per pixel, rows top to bottom) as a PNG file. */
  static encode(width: number, height: number, rgb: Uint8Array): Buffer {
    // Prefix every row with filter type 0
    const raw = new Uint8Array((width * 3 + 1) * height);
    for (let y = 0; y < height; y++) {
      raw.set(rgb.subarray(y * width * 3, (y + 1) * width * 3), y * (width * 3 + 1) + 1);
    }

    // 8-bit truecolour header
    const header = new Uint8Array(13);
    const view = new DataView(header.buffer);
    view.setUint32(0, width);
    view.setUint32(4, height);
    header[8] = 8;
    header[9] = 2;

    return Buffer.concat([
      new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]),
      Png.chunk("IHDR", header),
      Png.chunk("IDAT", deflateSync(raw, { level: 9 })),
      Png.chunk("IEND", new Uint8Array()),
    ]);
  }

  /** One length-prefixed, CRC-suffixed chunk. */
  private static chunk(type: string, data: Uint8Array): Uint8Array {
    const out = new Uint8Array(12 + data.length);
    const view = new DataView(out.buffer);
    view.setUint32(0, data.length);
    out.set(new TextEncoder().encode(type), 4);
    out.set(data, 8);
    view.setUint32(8 + data.length, Png.crc(out.subarray(4, 8 + data.length)));
    return out;
  }

  /** CRC-32 over a chunk's type and data. */
  private static crc(bytes: Uint8Array): number {
    let c = 0xffffffff;
    for (const b of bytes) c = (Png.crcTable[(c ^ b) & 255] ?? 0) ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  }
}

new DmgBackground(new DarkTheme(join(ROOT, "docs/design-system/tokens.json"))).build();
