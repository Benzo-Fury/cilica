/**
 * Panel open/closed state and widths. Every launch starts with both panels closed; only the
 * widths are remembered across launches.
 *
 * The canvas never gets narrower than CANVAS_MIN: when both panels would squeeze it, the one
 * opened earlier closes (see `panelToClose`), and resizing stops short of it.
 */
import { create } from "zustand";
import { persist } from "zustand/middleware";

export const SIDEBAR = { default: 248, min: 200, max: 360 } as const;
export const ASSISTANT = { default: 380, min: 320, max: 520 } as const;
/** The narrowest the canvas may get. Either panel alone always fits beside it (window min 880). */
export const CANVAS_MIN = 360;

type Panel = "sidebar" | "assistant";

interface PanelState {
  sidebarOpen: boolean;
  assistantOpen: boolean;
  sidebarWidth: number;
  assistantWidth: number;
  /** The panel opened most recently; the other one gives way when both do not fit. */
  lastOpened: Panel;
  toggleSidebar: () => void;
  toggleAssistant: () => void;
  setSidebarOpen: (open: boolean) => void;
  setAssistantOpen: (open: boolean) => void;
  setSidebarWidth: (width: number) => void;
  setAssistantWidth: (width: number) => void;
}

export const usePanels = create<PanelState>()(
  persist(
    (set) => ({
      sidebarOpen: false,
      assistantOpen: false,
      sidebarWidth: SIDEBAR.default,
      assistantWidth: ASSISTANT.default,
      lastOpened: "sidebar",
      toggleSidebar: () =>
        set((s) => ({
          sidebarOpen: !s.sidebarOpen,
          ...(!s.sidebarOpen && { lastOpened: "sidebar" }),
        })),
      toggleAssistant: () =>
        set((s) => ({
          assistantOpen: !s.assistantOpen,
          ...(!s.assistantOpen && { lastOpened: "assistant" }),
        })),
      setSidebarOpen: (sidebarOpen) =>
        set({ sidebarOpen, ...(sidebarOpen && { lastOpened: "sidebar" as const }) }),
      setAssistantOpen: (assistantOpen) =>
        set({ assistantOpen, ...(assistantOpen && { lastOpened: "assistant" as const }) }),
      setSidebarWidth: (sidebarWidth) => set({ sidebarWidth }),
      setAssistantWidth: (assistantWidth) => set({ assistantWidth }),
    }),
    {
      name: "cilica.panels",
      version: 4,
      // v4: open/closed is no longer stored, so every launch starts with both panels closed.
      migrate: (persisted) => {
        const { sidebarWidth, assistantWidth } = (persisted ?? {}) as Partial<PanelState>;
        return { sidebarWidth, assistantWidth } as never;
      },
      partialize: ({ sidebarWidth, assistantWidth }) => ({ sidebarWidth, assistantWidth }),
    },
  ),
);

/**
 * The panel to close so the canvas keeps CANVAS_MIN, or null when everything fits. Widths are
 * null for a panel that is shut. The one opened earlier gives way.
 */
export function panelToClose(
  windowWidth: number,
  sidebar: number | null,
  assistant: number | null,
  lastOpened: Panel,
): Panel | null {
  if (sidebar === null || assistant === null) return null;
  if (sidebar + assistant + CANVAS_MIN <= windowWidth) return null;
  return lastOpened === "sidebar" ? "assistant" : "sidebar";
}

/** The widest a panel may be dragged, leaving the canvas CANVAS_MIN beside the other panel. */
export function maxPanelWidth(
  max: number,
  min: number,
  windowWidth: number,
  other: number,
): number {
  return Math.max(min, Math.min(max, windowWidth - other - CANVAS_MIN));
}
