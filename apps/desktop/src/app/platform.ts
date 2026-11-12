/** Which OS the UI runs on. Decides shortcut glyphs, window chrome and material. */
export type Os = "macos" | "windows" | "linux";

export function detectOs(userAgent: string = globalThis.navigator?.userAgent ?? ""): Os {
  if (/Mac|iPhone|iPad/.test(userAgent)) return "macos";
  if (/Win/.test(userAgent)) return "windows";
  return "linux";
}

export const os: Os = detectOs();
export const isMac = os === "macos";
