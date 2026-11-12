/** App-wide keybind handlers (the registry decides the keys). */
import { useQueryClient } from "@tanstack/react-query";
import { useAction } from "@/app/keybinds";
import { qk } from "@/app/queryKeys";
import { navigate, openSettings, useRouter } from "@/app/router";
import { useCreateNote } from "@/features/notes/hooks";
import { useToggleRecording } from "@/features/recorder/hooks";
import { useSidebarUi } from "@/features/sidebar/store";
import type { NoteSummary } from "@/ipc/types";
import { useOverlays } from "./overlays";
import { usePanels } from "./panels";

export function useGlobalActions() {
  const createNote = useCreateNote();
  const queryClient = useQueryClient();
  const toggleRecording = useToggleRecording();

  useAction("sidebar.toggle", () => usePanels.getState().toggleSidebar());
  useAction("assistant.toggle", () => usePanels.getState().toggleAssistant());
  useAction("palette.open", () => useOverlays.getState().toggle("palette"));
  useAction("cheatsheet.open", () => useOverlays.getState().toggle("cheatsheet"));
  useAction("settings.open", () => {
    const { view } = useRouter.getState();
    if (view.name !== "settings") openSettings("general");
  });
  useAction("trash.open", () => navigate({ name: "trash" }));
  useAction("people.open", () => navigate({ name: "people" }));
  useAction("graph.open", () => navigate({ name: "graph" }));
  useAction("tasks.open", () => navigate({ name: "tasks" }));
  useAction("activity.open", () => navigate({ name: "activity" }));
  useAction("recording.toggle", () => void toggleRecording("hotkey"));
  useAction("note.new", () => {
    const { view } = useRouter.getState();
    // A new note lands in the open note's category, like Finder's New Folder.
    const notes = queryClient.getQueryData<NoteSummary[]>(qk.notes);
    const open = view.name === "note" ? notes?.find((n) => n.id === view.noteId) : undefined;
    createNote.mutate(open?.category_id ?? null);
  });
  useAction("category.new", () => {
    usePanels.getState().setSidebarOpen(true);
    useSidebarUi.getState().startNewCategory();
  });
}
