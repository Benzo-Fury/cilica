/**
 * The three-pane window: glass sidebar (248px, ⌘\), the opaque inset canvas, and the assistant
 * (380px, ⌘J) which pushes the canvas rather than covering it, or the floating bar instead
 * (switched with the buttons in Margin's header). Panels start closed; their widths persist. The
 * canvas keeps CANVAS_MIN: when both panels do not fit, the one opened earlier closes.
 *
 * At launch the empty page holds the window alone for a moment, then the chrome comes in (intro.ts).
 */
import { useEffect, useState } from "react";
import { AssistantPanel } from "@/features/assistant/AssistantPanel";
import { useMarginPlacement } from "@/features/assistant/placement";
import { CheatSheet } from "@/features/palette/CheatSheet";
import { CommandPalette } from "@/features/palette/CommandPalette";
import { ConsentNotice } from "@/features/recorder/ConsentNotice";
import { useRecordingEffects } from "@/features/recorder/hooks";
import { useModelDownloadEvents } from "@/features/settings/system";
import { Sidebar } from "@/features/sidebar/Sidebar";
import { useLiveTranscripts } from "@/features/transcript/hooks";
import { useBackgroundUpdates } from "@/features/updates/hooks";
import { haptic } from "@/lib/haptics";
import { CanvasSheet, ResizeHandle, SidePanel, WindowFrame } from "@/ui";
import { Canvas } from "./Canvas";
import { useLaunchIntro } from "./intro";
import { ASSISTANT, maxPanelWidth, panelToClose, SIDEBAR, usePanels } from "./panels";
import { useTitleBarDoubleClick } from "./titleBarDoubleClick";
import { useGlobalActions } from "./useGlobalActions";
import { useWindowWidth } from "./useWindowWidth";

export function Shell() {
  useGlobalActions();
  useTitleBarDoubleClick();
  useRecordingEffects();
  useLiveTranscripts();
  useModelDownloadEvents();
  useBackgroundUpdates();
  const intro = useLaunchIntro();
  const holding = intro === "hold";
  // Panels stay shut while the intro holds; opening them after is what slides them in.
  const sidebarOpen = usePanels((s) => s.sidebarOpen) && !holding;
  const sidePanel = useMarginPlacement() === "side_panel";
  // With the floating bar, Margin opens inside the canvas and the right panel stays shut.
  const assistantOpen = usePanels((s) => s.assistantOpen) && sidePanel && !holding;
  const sidebarWidth = usePanels((s) => s.sidebarWidth);
  const assistantWidth = usePanels((s) => s.assistantWidth);
  const setSidebarWidth = usePanels((s) => s.setSidebarWidth);
  const setAssistantWidth = usePanels((s) => s.setAssistantWidth);
  const [resizing, setResizing] = useState<"left" | "right" | null>(null);
  const windowWidth = useWindowWidth();
  const lastOpened = usePanels((s) => s.lastOpened);

  // Both panels and the canvas must fit: the panel opened earlier closes when they do not.
  useEffect(() => {
    const close = panelToClose(
      windowWidth,
      sidebarOpen ? sidebarWidth : null,
      assistantOpen ? assistantWidth : null,
      lastOpened,
    );
    if (close === "sidebar") usePanels.getState().setSidebarOpen(false);
    if (close === "assistant") usePanels.getState().setAssistantOpen(false);
  }, [windowWidth, sidebarOpen, assistantOpen, sidebarWidth, assistantWidth, lastOpened]);

  return (
    <WindowFrame
      data-resizing={resizing ?? undefined}
      data-intro={intro === "done" ? undefined : intro}
    >
      <SidePanel
        side="left"
        open={sidebarOpen}
        width={sidebarWidth}
        resizing={resizing === "left"}
        label="Sidebar"
      >
        <Sidebar />
      </SidePanel>
      <CanvasSheet
        gutter={
          <>
            {sidebarOpen && (
              <ResizeHandle
                side="left"
                label="Resize sidebar"
                width={sidebarWidth}
                min={SIDEBAR.min}
                max={maxPanelWidth(
                  SIDEBAR.max,
                  SIDEBAR.min,
                  windowWidth,
                  assistantOpen ? assistantWidth : 0,
                )}
                snapTo={SIDEBAR.default}
                onResizeStart={() => setResizing("left")}
                onResize={setSidebarWidth}
                onResizeEnd={(w) => {
                  if (w === SIDEBAR.default) haptic("alignment");
                  setResizing(null);
                }}
              />
            )}
            {assistantOpen && (
              <ResizeHandle
                side="right"
                label="Resize assistant"
                width={assistantWidth}
                min={ASSISTANT.min}
                max={maxPanelWidth(
                  ASSISTANT.max,
                  ASSISTANT.min,
                  windowWidth,
                  sidebarOpen ? sidebarWidth : 0,
                )}
                snapTo={ASSISTANT.default}
                onResizeStart={() => setResizing("right")}
                onResize={setAssistantWidth}
                onResizeEnd={(w) => {
                  if (w === ASSISTANT.default) haptic("alignment");
                  setResizing(null);
                }}
              />
            )}
          </>
        }
      >
        <Canvas />
      </CanvasSheet>
      <SidePanel
        side="right"
        open={assistantOpen}
        width={assistantWidth}
        resizing={resizing === "right"}
        label="Assistant"
      >
        {sidePanel && <AssistantPanel />}
      </SidePanel>
      <CommandPalette />
      <CheatSheet />
      <ConsentNotice />
    </WindowFrame>
  );
}
