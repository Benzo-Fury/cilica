/**
 * Image attachments for the browser preview and tests: bytes kept in memory, named like the
 * real store (`attachments/<32 hex>.<ext>`), shown through `blob:` URLs.
 */
import type { ImageTextStatus, SavedImage } from "@/ipc/types";
import type { MockContext } from "./state";
import { appError } from "./util";

const files = new Map<string, { blob: Blob; url: string | null }>();

const SIGNATURES: [string, number[]][] = [
  ["png", [0x89, 0x50, 0x4e, 0x47]],
  ["jpg", [0xff, 0xd8, 0xff]],
  ["gif", [0x47, 0x49, 0x46, 0x38]],
  ["bmp", [0x42, 0x4d]],
];

function sniff(bytes: Uint8Array): string | null {
  for (const [ext, sig] of SIGNATURES) if (sig.every((b, i) => bytes[i] === b)) return ext;
  const riff = String.fromCharCode(...bytes.slice(0, 4));
  const webp = String.fromCharCode(...bytes.slice(8, 12));
  if (riff === "RIFF" && webp === "WEBP") return "webp";
  if (String.fromCharCode(...bytes.slice(4, 12)).startsWith("ftypavif")) return "avif";
  return null;
}

/** 128 bits of FNV-1a in four lanes: stable names for the same bytes, like the real hash. */
function hash(bytes: Uint8Array): string {
  const lanes = [0x811c9dc5, 0x01000193, 0x9e3779b9, 0x85ebca6b];
  for (let i = 0; i < bytes.length; i++) {
    const lane = i % 4;
    lanes[lane] = Math.imul((lanes[lane] as number) ^ (bytes[i] as number), 0x01000193) >>> 0;
  }
  return lanes.map((n) => n.toString(16).padStart(8, "0")).join("");
}

async function dimensions(blob: Blob): Promise<{ width: number; height: number }> {
  try {
    const bitmap = await createImageBitmap(blob);
    const size = { width: bitmap.width, height: bitmap.height };
    bitmap.close();
    return size;
  } catch {
    return { width: 0, height: 0 };
  }
}

async function store(bytes: Uint8Array, name: string | null): Promise<SavedImage> {
  const ext = sniff(bytes);
  if (!ext) throw appError("invalid", "This file isn't an image Cilica can read.");
  const path = `attachments/${hash(bytes)}.${ext}`;
  const type = ext === "jpg" ? "image/jpeg" : `image/${ext}`;
  const blob = new Blob([bytes as BlobPart], { type });
  if (!files.has(path)) files.set(path, { blob, url: null });
  const size = await dimensions(blob);
  return { path, ...size, name: (name ?? "").replace(/\.[^.]+$/, "") || "Image" };
}

export function mockAttachmentUrl(path: string): string | null {
  const file = files.get(path);
  if (!file) return null;
  if (!file.url && typeof URL.createObjectURL === "function")
    file.url = URL.createObjectURL(file.blob);
  return file.url;
}

/** Images the preview pretends it has read: every one, so the setting shows as finished. */
const MOCK_IMAGES_READ = 12;

export function attachmentsCommands(ctx: MockContext) {
  return {
    imageTextStatus(): ImageTextStatus {
      return {
        supported: true,
        enabled: ctx.state.settings.writing.find_text_in_images,
        done: MOCK_IMAGES_READ,
        total: MOCK_IMAGES_READ,
        running: false,
        paused: false,
      };
    },
    async saveImage(data: string, name: string | null): Promise<SavedImage> {
      const binary = atob(data);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
      return store(bytes, name);
    },
    importImageFile(_path: string): SavedImage {
      throw appError(
        "unavailable",
        "The browser preview can't read files by path. Paste the image or drop it onto the note instead.",
      );
    },
    async fetchImage(url: string): Promise<SavedImage> {
      let response: Response;
      try {
        response = await fetch(url);
      } catch {
        throw appError(
          "upstream",
          "Cilica couldn't download that image. Save it and drag it into the note instead.",
        );
      }
      if (!response.ok)
        throw appError(
          "upstream",
          `Cilica couldn't download that image: the site answered ${response.status}.`,
        );
      const name = new URL(url).pathname.split("/").pop() ?? null;
      return store(new Uint8Array(await response.arrayBuffer()), name);
    },
    attachmentsDir(): string {
      return "/mock/attachments";
    },
    saveImageCopy(_path: string, _dest: string): null {
      return null;
    },
  };
}
