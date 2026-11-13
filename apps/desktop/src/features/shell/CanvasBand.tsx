import { ArrowLeft, PanelLeft, PanelRight } from "lucide-react";
import type { ReactNode } from "react";
import { useShortcut } from "@/app/keybinds";
import { isMac } from "@/app/platform";
import { useRouter } from "@/app/router";
import { useCilicaPlacement } from "@/features/assistant/placement";
import { isTauri } from "@/ipc/client";
import { CanvasBandLayout, IconButton, TrafficLightsSpace, WindowControls } from "@/ui";
import { usePanels } from "./panels";
import { useWindowControls } from "./useWindowControls";

/**
 * The band at the top of every canvas view. It brings back the sidebar toggle (and leaves room
 * for the traffic lights) when the sidebar is closed, and, while Cilica's side panel is hidden, ends with the button that
 * shows it (the panel carries its own hide button; the floating bar needs neither).
 *
 * Any view with a breadcrumb gets a back button before it that returns to the view you came from
 * (home when there is none). A view that has its own way back (a task report) passes `back={false}`.
 */
export function CanvasBand({
  start,
  center,
  end,
  back = Boolean(start),
}: {
  /** Show the back button that closes this view. Defaults to on whenever there is a breadcrumb. */
  back?: boolean;
  start?: ReactNode;
  center?: ReactNode;
  end?: ReactNode;
}) {
  const sidebarOpen = usePanels((s) => s.sidebarOpen);
  const sidePanel = useCilicaPlacement() === "side_panel";
  const assistantOpen = usePanels((s) => s.assistantOpen) && sidePanel;
  const toggleSidebar = usePanels((s) => s.toggleSidebar);
  const toggleAssistant = usePanels((s) => s.toggleAssistant);
  const sidebarKeys = useShortcut("sidebar.toggle");
  const assistantKeys = useShortcut("assistant.toggle");
  const controls = useWindowControls();
  const goBack = useRouter((s) => s.back);
  return (
    <CanvasBandLayout
      start={
        <>
          {!sidebarOpen && (
            <>
              {isMac && <TrafficLightsSpace preview={!isTauri} />}
              <IconButton
                icon={PanelLeft}
                iconMotion="sidebar"
                label="Show sidebar"
                shortcut={sidebarKeys}
                onClick={toggleSidebar}
              />
            </>
          )}
          {back && (
            <IconButton
              icon={ArrowLeft}
              iconMotion="arrow"
              label="Back"
              onClick={goBack}
            />
          )}
          {start}
        </>
      }
      center={center}
      end={
        <>
          {end}
          {sidePanel && !assistantOpen && (
            <IconButton
              icon={PanelRight}
              label="Ask Cilica"
              shortcut={assistantKeys}
              onClick={toggleAssistant}
            />
          )}
          {!isMac && !assistantOpen && <WindowControls {...controls} />}
        </>
      }
    />
  );
}
