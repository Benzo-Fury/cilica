/**
 * Saving images into the attachments folder, for the editor's image input and the image actions.
 * Every image that reaches a note goes through here or the backend's own sweep, so a note never
 * keeps image data in its text or loads anything from the web.
 */
import { attachmentFile } from "@/ipc/attachments";
import { api, errorMessage, isTauri } from "@/ipc/client";
import type { SavedImage } from "@/ipc/types";
import { toast } from "@/lib/toast";
import type { ImageSaver } from "../editor/imageInput";
import { IMAGE_EXTENSIONS } from "../editor/imageInput";

/** A File's bytes as base64 (what `save_image` takes). */
export function fileToBase64(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result ?? "");
      resolve(url.slice(url.indexOf(",") + 1));
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

export function saveImageFile(file: File): Promise<SavedImage> {
  return fileToBase64(file).then((data) => api.saveImage(data, file.name || null));
}

/** A web address is downloaded once; a `data:` URI is saved as it is. */
export function saveImageSource(src: string): Promise<SavedImage> {
  if (src.startsWith("data:")) {
    return api.saveImage(src.slice(src.indexOf(",") + 1), null);
  }
  return api.fetchImage(src);
}

export function reportImageError(error: unknown) {
  toast.error("Couldn't add the image", { description: errorMessage(error) });
}

export const imageSaver: ImageSaver = {
  file: saveImageFile,
  source: saveImageSource,
  failed: reportImageError,
};

/**
 * Ask for image files. Inside Tauri the system picker returns paths, which the backend reads;
 * in the browser preview a hidden file input returns the files themselves.
 */
export async function pickImages(multiple: boolean): Promise<SavedImage[]> {
  if (isTauri) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({
      title: multiple ? "Insert images" : "Replace image",
      multiple,
      filters: [{ name: "Images", extensions: IMAGE_EXTENSIONS }],
    });
    if (!picked) return [];
    const paths = Array.isArray(picked) ? picked : [picked];
    return Promise.all(paths.map((p) => api.importImageFile(p)));
  }
  const files = await new Promise<File[]>((resolve) => {
    const input = Object.assign(document.createElement("input"), {
      type: "file",
      accept: "image/*",
      multiple,
    });
    input.onchange = () => resolve(Array.from(input.files ?? []));
    input.oncancel = () => resolve([]);
    input.click();
  });
  return Promise.all(files.map(saveImageFile));
}

/** The absolute file for an attachment, for Save as and Show in Finder. */
export { attachmentFile };
