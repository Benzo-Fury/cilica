import { describe, expect, it } from "vitest";
import { paintMesa, paintOcean, paintPines, type SceneTones } from "@/ui/SceneArt";
import tokens from "../../../../../docs/design-system/tokens.json";
import { THEMES, themeLook, themeOption } from "./themes";

const looks = tokens.color.looks;

describe("themes", () => {
  it("falls back to System for a theme it does not know", () => {
    expect(themeOption(undefined).value).toBe("system");
  });

  it("sets a look for every theme except System, Light and Dark", () => {
    const named = THEMES.filter((theme) => themeLook(theme));
    expect(THEMES.filter((theme) => !themeLook(theme)).map((theme) => theme.value)).toEqual([
      "system",
      "light",
      "dark",
    ]);
    expect(named.map((theme) => themeLook(theme))).toEqual(named.map((theme) => theme.value));
  });

  it("matches the looks in tokens.json", () => {
    const named = THEMES.filter((theme) => themeLook(theme));
    expect(named.map((t) => ({ id: t.value, base: t.scheme, scene: t.scene }))).toEqual(
      looks.map((look) => ({ id: look.id, base: look.base, scene: look.scene })),
    );
  });
});

describe("scene painters", () => {
  const tones: SceneTones = { canvas: [244, 247, 241], ink: [27, 36, 28], tint: [95, 154, 107] };
  const blank = (width: number, height: number) =>
    ({ width, height, data: new Uint8ClampedArray(width * height * 4) }) as ImageData;

  it.each([
    ["pines", paintPines],
    ["mesa", paintMesa],
    ["ocean", paintOcean],
  ] as const)("%s fills every pixel and leaves the top row the page colour", (_, paint) => {
    const img = blank(120, 75);
    paint(img, tones);
    for (let o = 3; o < img.data.length; o += 4) expect(img.data[o]).toBe(255);
    expect([...img.data.slice(0, 3)]).toEqual(tones.canvas);
  });

  it("paints the sea by night under a dark theme: a sky lit only by its stars", () => {
    const night: SceneTones = { canvas: [14, 24, 32], ink: [227, 237, 242], tint: [60, 143, 196] };
    const [W, H] = [240, 150];
    const img = blank(W, H);
    paintOcean(img, night);
    for (let o = 3; o < img.data.length; o += 4) expect(img.data[o]).toBe(255);
    // The upper sky stays dark but for a scatter of stars.
    let stars = 0;
    for (let o = 0; o < W * Math.floor(H * 0.4) * 4; o += 4) {
      if ((img.data[o] ?? 0) + (img.data[o + 1] ?? 0) > 200) stars++;
    }
    expect(stars).toBeGreaterThan(0);
    expect(stars).toBeLessThan(W * H * 0.4 * 0.02);
  });
});
