/**
 * Text found in images (macOS): the indexing progress Settings › General shows, kept fresh by
 * the backend's progress events, and whether Cilica can read that text at all on this machine.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useBackendEvent } from "@/app/backendEvents";
import { qk } from "@/app/queryKeys";
import { api } from "@/ipc/client";
import type { ImageTextStatus } from "@/ipc/types";

export function useImageTextStatus() {
  const client = useQueryClient();
  useBackendEvent("image_text_progress", (event) => {
    client.setQueryData(qk.imageText, event.status);
  });
  return useQuery({ queryKey: qk.imageText, queryFn: () => api.imageTextStatus() });
}

/** Text in images is found on this machine and the setting is on. */
export function useReadsImageText(): boolean {
  const { data } = useImageTextStatus();
  return !!data?.supported && data.enabled;
}

/** "Finding text in 12 of 40 images", "Text found in all 40 images", or null when idle and empty. */
export function imageTextLine(status: ImageTextStatus): string | null {
  const { done, total } = status;
  if (total === 0) return "No images in your notes yet.";
  if (status.paused) return `Paused while recording. ${done} of ${total} images done.`;
  if (done >= total)
    return total === 1 ? "Done for your 1 image." : `Done for all ${total} images.`;
  if (!status.enabled) return `${done} of ${total} images done. Turn this on to do the rest.`;
  return `Looking at image ${Math.min(done + 1, total)} of ${total}…`;
}
