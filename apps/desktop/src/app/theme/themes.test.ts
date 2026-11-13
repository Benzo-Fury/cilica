import { describe, expect, it } from "vitest";
import { paintMesa, paintPines, type SceneTones } from "@/ui/SceneArt";
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
  ] as const)("%s fills every pixel and leaves the top row the page colour", (_, paint) => {
    const img = blank(120, 75);
    paint(img, tones);
    for (let o = 3; o < img.data.length; o += 4) expect(img.data[o]).toBe(255);
    expect([...img.data.slice(0, 3)]).toEqual(tones.canvas);
  });
});
