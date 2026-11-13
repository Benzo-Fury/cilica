import { FolderPlus, PanelLeft, Search, SquarePen } from "lucide-react";
import { LayoutGroup } from "motion/react";
import { runAction, useShortcut } from "@/app/keybinds";
import { isMac } from "@/app/platform";
import { usePanels } from "@/features/shell/panels";
import { isTauri } from "@/ipc/client";
import { Icon, IconButton, Kbd, TitleBand, TrafficLightsSpace } from "@/ui";
import { BottomSection } from "./BottomSection";
import { NoteTree } from "./NoteTree";
import { useSidebarUi } from "./store";

/** The left sidebar: new note, search (opens the palette), notes and categories, bottom items. */
export function Sidebar() {
  const toggleSidebar = usePanels((s) => s.toggleSidebar);
  const startNewCategory = useSidebarUi((s) => s.startNewCategory);
  const keys = {
    sidebar: useShortcut("sidebar.toggle"),
    newNote: useShortcut("note.new"),
    newCategory: useShortcut("category.new"),
    search: useShortcut("palette.open"),
  };

  return (
    <LayoutGroup id="sidebar">
      <div className="nt-sidebar-inner" data-sidebar>
        <TitleBand className="nt-sidebar-band">
          {isMac && <TrafficLightsSpace preview={!isTauri} />}
          {isMac && <span className="nt-band-spacer" data-tauri-drag-region />}
          <IconButton
            icon={FolderPlus}
            iconMotion="folder"
            label="New category"
            shortcut={keys.newCategory}
            onClick={startNewCategory}
          />
          <IconButton
            icon={SquarePen}
            label="New note"
            shortcut={keys.newNote}
            onClick={() => runAction("note.new")}
          />
          <IconButton
            icon={PanelLeft}
            iconMotion="sidebar"
            label="Hide sidebar"
            shortcut={keys.sidebar}
            onClick={toggleSidebar}
          />
          {/* On Windows the window controls sit top right, so the buttons start at the left. */}
          {!isMac && <span className="nt-band-spacer" data-tauri-drag-region />}
        </TitleBand>
        <button
          type="button"
          className="nt-sidebar-searchbtn"
          onClick={() => runAction("palette.open")}
          aria-keyshortcuts={keys.search.join("+")}
        >
          <Icon icon={Search} size="sm" />
          <span>Search</span>
          <Kbd keys={keys.search} />
        </button>
        <nav className="nt-sidebar-scroll" aria-label="Notes">
          <NoteTree />
        </nav>
        <BottomSection />
      </div>
    </LayoutGroup>
  );
}
