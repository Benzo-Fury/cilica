/**
 * Assistant UI state shared with the rest of the app: the open thread and a draft. The palette's
 * "Ask Cilica" row (⌘↩) hands its query over through `askCilica`; it lands in the composer.
 */
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { usePanels } from "../shell/panels";

interface AssistantState {
  /** Text waiting in the composer. */
  draft: string;
  /** Bumped to move focus into the composer (⌘J, Ask Cilica). */
  focusTick: number;
  /** The conversation on screen; `null` shows the list, and the first send starts a new one. */
  threadId: string | null;
  /** Conversations deleted while their Undo toast is up: hidden now, deleted when it closes. */
  hidden: string[];
  setDraft: (draft: string) => void;
  focusComposer: () => void;
  setThreadId: (threadId: string | null) => void;
  hide: (threadId: string) => void;
  unhide: (threadId: string) => void;
}

export const useAssistantStore = create<AssistantState>()(
  persist(
    (set) => ({
      draft: "",
      focusTick: 0,
      threadId: null,
      hidden: [],
      setDraft: (draft) => set({ draft }),
      focusComposer: () => set((s) => ({ focusTick: s.focusTick + 1 })),
      setThreadId: (threadId) => set({ threadId }),
      hide: (threadId) => set((s) => ({ hidden: [...s.hidden, threadId] })),
      unhide: (threadId) => set((s) => ({ hidden: s.hidden.filter((id) => id !== threadId) })),
    }),
    { name: "cilica.assistant", version: 1, partialize: ({ threadId }) => ({ threadId }) },
  ),
);

/** Open the assistant with a question ready to send. */
export function askCilica(question: string) {
  useAssistantStore.getState().setDraft(question);
  usePanels.getState().setAssistantOpen(true);
  useAssistantStore.getState().focusComposer();
}
