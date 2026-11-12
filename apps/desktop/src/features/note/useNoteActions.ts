/** Export and copy for the note overflow menu. */
import { useCallback } from "react";
import { api, errorMessage, isTauri } from "@/ipc/client";
import { displayTitle } from "@/lib/format";
import { toast } from "@/lib/toast";

const fileName = (title: string) =>
  `${displayTitle(title)
    .replace(/[\\/:*?"<>|]+/g, "-")
    .slice(0, 80)}.md`;

export function useNoteActions(
  noteId: string,
  title: string,
  markdown: () => string,
  flush: () => Promise<void>,
) {
  const exportMarkdown = useCallback(async () => {
    await flush();
    try {
      if (isTauri) {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const path = await save({
          title: "Export as Markdown",
          defaultPath: fileName(title),
          filters: [{ name: "Markdown", extensions: ["md"] }],
        });
        if (!path) return;
        await api.exportNoteMarkdown(noteId, path);
        toast.success(`Exported ${displayTitle(title)}`, { description: path });
      } else {
        // Browser preview: hand the file to the browser.
        const url = URL.createObjectURL(new Blob([markdown()], { type: "text/markdown" }));
        const a = Object.assign(document.createElement("a"), {
          href: url,
          download: fileName(title),
        });
        a.click();
        URL.revokeObjectURL(url);
        toast.success(`Exported ${displayTitle(title)}`);
      }
    } catch (error) {
      toast.error("The note was not exported", { description: errorMessage(error) });
    }
  }, [noteId, title, markdown, flush]);

  const copyMarkdown = useCallback(async () => {
    await flush();
    try {
      await navigator.clipboard.writeText(markdown());
      toast.success("Copied as Markdown");
    } catch {
      toast.error("Nothing was copied", {
        description: "The clipboard is not available. Try again.",
      });
    }
  }, [markdown, flush]);

  return { exportMarkdown, copyMarkdown };
}
