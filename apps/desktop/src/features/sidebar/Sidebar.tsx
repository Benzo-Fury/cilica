import { FolderPlus, PanelLeft, Search, SquarePen } from "lucide-react";
import { LayoutGroup } from "motion/react";
import { runAction, useShortcut } from "@/app/keybinds";
import { isMac } from "@/app/platform";
import { openNote, useRouter } from "@/app/router";
import { isLive, useRecordingState } from "@/features/recorder/hooks";
import { usePanels } from "@/features/shell/panels";
import { isTauri } from "@/ipc/client";
import { clock } from "@/lib/format";
import { Icon, IconButton, Kbd, RecDot, Row, TitleBand, TrafficLightsSpace } from "@/ui";
import { BottomSection } from "./BottomSection";
import { NoteTree } from "./NoteTree";
import { SIDEBAR_PILL } from "./rows";
import { useSidebarUi } from "./store";
import { useNow } from "./useNow";

function RecordingNowRow() {
  const recording = useRecordingState();
  const currentNote = useRouter((s) => (s.view.name === "note" ? s.view.noteId : null));
  const now = useNow(isLive(recording));
  if (!isLive(recording) || !recording.note_id) return null;
  const started = recording.started_at ? new Date(recording.started_at).getTime() : now;
  const noteId = recording.note_id;
  return (
    <Row
      role="button"
      tabIndex={0}
      leading={
        <span className="nt-row-dot">
          <RecDot />
        </span>
      }
      label="Recording now"
      meta={<span className="nt-mono">{clock(now - started)}</span>}
      current={currentNote === noteId}
      pillId={SIDEBAR_PILL}
      onClick={() => openNote(noteId)}
    />
  );
}

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
          <span className="nt-band-spacer" data-tauri-drag-region />
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
          <RecordingNowRow />
          <NoteTree />
        </nav>
        <BottomSection />
      </div>
    </LayoutGroup>
  );
}
