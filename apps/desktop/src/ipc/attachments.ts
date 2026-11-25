/**
 * Where a note's image files are, as URLs the webview can load.
 *
 * Notes refer to images as `attachments/<hash>.<ext>`. Inside Tauri that file sits in the app
 * data folder and loads through the asset protocol; in the browser preview the mock keeps the
 * bytes in memory and hands out `blob:` URLs.
 */
import { api, isTauri } from "./client";

/** `attachments/<32 hex>.<ext>`: the only paths the app writes or resolves. */
const ATTACHMENT = /^attachments\/[0-9a-f]{32}\.(png|jpg|gif|webp|avif|bmp)$/;

export function isAttachmentPath(src: string | null | undefined): src is string {
  return !!src && ATTACHMENT.test(src);
}

let folder: Promise<string> | null = null;
let folderNow: string | null = null;

/** The absolute attachments folder (Tauri only). */
export function attachmentsFolder(): Promise<string> {
  if (folder) return folder;
  const next = api.attachmentsDir().then((dir) => {
    folderNow = dir.replace(/[\\/]+$/, "");
    return folderNow;
  });
  // A failure is not remembered: the next image asks again.
  next.catch(() => {
    if (folder === next) folder = null;
  });
  folder = next;
  return next;
}

/** The file on disk for `attachments/<name>`. */
export async function attachmentFile(path: string): Promise<string> {
  return join(await attachmentsFolder(), path);
}

/** The folder and the file name, with the folder's own separator (Windows uses `\`). */
function join(dir: string, path: string): string {
  const sep = dir.includes("\\") && !dir.includes("/") ? "\\" : "/";
  return `${dir}${sep}${path.slice("attachments/".length)}`;
}

let convert: ((path: string) => string) | null = null;

/** A URL the webview can load, or null if this is not an attachment the app knows. */
export async function attachmentUrl(path: string): Promise<string | null> {
  if (!isAttachmentPath(path)) return null;
  if (!isTauri) {
    const { mockAttachmentUrl } = await import("./mock/attachments");
    return mockAttachmentUrl(path);
  }
  if (!convert) convert = (await import("@tauri-apps/api/core")).convertFileSrc;
  return convert(await attachmentFile(path));
}

/** The same, without waiting, once the folder is known (first paint of an open note). */
export function attachmentUrlNow(path: string): string | null {
  if (!isTauri || !folderNow || !convert || !isAttachmentPath(path)) return null;
  return convert(join(folderNow, path));
}
