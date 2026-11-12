/**
 * The command palette (⌘K): notes, transcript matches, actions from the keybind registry and
 * settings pages, on strong glass over a scrim. ⌘↩ hands the query to Margin.
 */
import {
  Activity,
  AudioLines,
  CalendarClock,
  FileText,
  FolderPlus,
  History,
  Keyboard,
  type LucideIcon,
  MessageCircle,
  Mic,
  NotebookPen,
  PanelLeft,
  PanelRight,
  Plus,
  Search,
  SearchCheck,
  Settings,
  SquarePen,
  Trash2,
  Users,
  Waypoints,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  ACTIONS,
  type ActionId,
  canRunAction,
  effectiveKeys,
  eventToAccelerator,
  formatAccelerator,
  runAction,
  useKeybindStore,
} from "@/app/keybinds";
import { isMac } from "@/app/platform";
import { openNote, openSettings } from "@/app/router";
import { askMargin } from "@/features/assistant/store";
import { useCategories, useNotes } from "@/features/notes/hooks";
import { ALL_SETTINGS_PAGES } from "@/features/settings/pages";
import { useOverlays } from "@/features/shell/overlays";
import { useSearchNotes } from "@/features/sidebar/useSearch";
import { clock, displayTitle, relativeDate } from "@/lib/format";
import {
  DialogRoot,
  DialogSurface,
  Excerpt,
  Highlight,
  PaletteEmpty,
  PaletteFooter,
  PaletteGroup,
  PaletteInput,
  PaletteItem,
  PaletteList,
  PaletteNote,
  PalettePhrase,
  PaletteRoot,
} from "@/ui";
import { saidBy, useNearWords, whereSaid } from "./near";

const ACTION_ICONS: Partial<Record<ActionId, LucideIcon>> = {
  "note.new": Plus,
  "category.new": FolderPlus,
  "note.find": Search,
  "note.togglePanel": AudioLines,
  "note.history": History,
  "note.trash": Trash2,
  "recording.toggle": Mic,
  "assistant.toggle": PanelRight,
  "sidebar.toggle": PanelLeft,
  "settings.open": Settings,
  "cheatsheet.open": Keyboard,
  "trash.open": Trash2,
  "assistant.newThread": SquarePen,
  "people.open": Users,
  "graph.open": Waypoints,
  "tasks.open": CalendarClock,
  "activity.open": Activity,
};

/** Every word of the query appears in the text (case-insensitive). */
function matches(text: string, query: string): boolean {
  const hay = text.toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}

export function CommandPalette() {
  const open = useOverlays((s) => s.open === "palette");
  const close = useOverlays((s) => s.close);
  return (
    <DialogRoot open={open} onOpenChange={(o) => !o && close()}>
      {open && <PaletteBody onClose={close} />}
    </DialogRoot>
  );
}

function PaletteBody({ onClose }: { onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState("");
  const input = useRef<HTMLInputElement>(null);
  const overrides = useKeybindStore((s) => s.overrides);
  const { data: notes = [] } = useNotes();
  const { data: categories = [] } = useCategories();
  const search = useSearchNotes(query, 20);
  const hits = search.data ?? [];
  const q = query.trim();

  const categoryName = (id: string | null) => categories.find((c) => c.id === id)?.name ?? "Notes";

  const noteResults = useMemo(() => {
    if (!q) return [...notes].sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, 6);
    return notes.filter((n) => matches(displayTitle(n.title), q)).slice(0, 8);
  }, [notes, q]);

  const shownIds = new Set(noteResults.map((n) => n.id));
  const bodyHits = q
    ? hits.filter((h) => h.source === "body" && !shownIds.has(h.note.id)).slice(0, 5)
    : [];
  const transcriptHits = q ? hits.filter((h) => h.source === "transcript").slice(0, 4) : [];

  const actions = ACTIONS.filter(
    (a) => "inPalette" in a && a.inPalette && canRunAction(a.id) && (!q || matches(a.label, q)),
  );
  const pages = ALL_SETTINGS_PAGES.filter(
    (p) => !q || matches(`Settings ${p.label} ${p.keywords.join(" ")}`, q),
  );

  // Nothing anywhere: say so, and offer what was said that comes closest.
  const settled = search.isSuccess && !search.isPlaceholderData && !search.isFetching;
  const nothing =
    q.length > 0 &&
    settled &&
    noteResults.length + hits.length + actions.length + pages.length === 0;
  const { data: near = [] } = useNearWords(q, nothing);

  const run = (fn: () => void) => {
    onClose();
    // Let the dialog close and return focus before the action moves it.
    requestAnimationFrame(fn);
  };

  useEffect(() => {
    input.current?.focus();
  }, []);

  const keys = (k: string) => formatAccelerator(k, isMac);

  return (
    <DialogSurface
      variant="glass"
      placement="top"
      title="Command palette"
      className="nt-palette-dialog"
    >
      <PaletteRoot
        label="Command palette"
        shouldFilter={false}
        loop
        value={selected}
        onValueChange={setSelected}
        onKeyDown={(event) => {
          if (eventToAccelerator(event.nativeEvent, isMac) === "Mod+Enter" && q) {
            event.preventDefault();
            run(() => askMargin(q));
          }
        }}
      >
        <PaletteInput
          ref={input}
          icon={Search}
          value={query}
          onValueChange={setQuery}
          placeholder="Search or run a command"
          escKeys={keys("Escape")}
        />
        <PaletteList>
          <PaletteEmpty>No results for “{q}”.</PaletteEmpty>
          {nothing && (
            <PaletteNote>Nobody has said “{q}” in a meeting, and no note mentions it.</PaletteNote>
          )}
          {nothing && near.length > 0 && (
            <PaletteGroup heading="Close">
              {near.map((w) => (
                <PaletteItem
                  key={w.phrase}
                  value={`near:${w.phrase}`}
                  selected={selected === `near:${w.phrase}`}
                  icon={SearchCheck}
                  hint={whereSaid(w)}
                  onSelect={() => {
                    setQuery(w.phrase);
                    input.current?.focus();
                  }}
                >
                  <PalettePhrase>{w.phrase}</PalettePhrase>
                  {saidBy(w)}
                </PaletteItem>
              ))}
            </PaletteGroup>
          )}
          {noteResults.length > 0 && (
            <PaletteGroup heading={q ? "Notes" : "Recent notes"}>
              {noteResults.map((note) => (
                <PaletteItem
                  key={note.id}
                  value={`note:${note.id}`}
                  selected={selected === `note:${note.id}`}
                  icon={note.kind === "meeting" ? Mic : FileText}
                  hint={`${categoryName(note.category_id)} · ${relativeDate(note.updated_at)}`}
                  onSelect={() => run(() => openNote(note.id))}
                >
                  <Highlight text={displayTitle(note.title)} query={q} />
                </PaletteItem>
              ))}
              {bodyHits.map((hit) => (
                <PaletteItem
                  key={`body:${hit.note.id}`}
                  value={`body:${hit.note.id}`}
                  selected={selected === `body:${hit.note.id}`}
                  icon={NotebookPen}
                  hint={categoryName(hit.note.category_id)}
                  onSelect={() => run(() => openNote(hit.note.id))}
                  sub={<Excerpt excerpt={hit.excerpt} />}
                >
                  {displayTitle(hit.note.title)}
                </PaletteItem>
              ))}
            </PaletteGroup>
          )}
          {transcriptHits.length > 0 && (
            <PaletteGroup heading="Transcript matches">
              {transcriptHits.map((hit) => {
                const value = `tx:${hit.note.id}:${hit.at_ms ?? 0}`;
                return (
                  <PaletteItem
                    key={value}
                    value={value}
                    selected={selected === value}
                    icon={AudioLines}
                    hint={
                      hit.at_ms !== null ? (
                        <span className="nt-mono">{clock(hit.at_ms)}</span>
                      ) : undefined
                    }
                    onSelect={() =>
                      run(() =>
                        openNote(hit.note.id, {
                          panel: "transcript",
                          atMs: hit.at_ms ?? undefined,
                        }),
                      )
                    }
                  >
                    <Excerpt excerpt={hit.excerpt} />
                  </PaletteItem>
                );
              })}
            </PaletteGroup>
          )}
          {actions.length > 0 && (
            <PaletteGroup heading="Commands">
              {actions.map((action) => (
                <PaletteItem
                  key={action.id}
                  value={`action:${action.id}`}
                  selected={selected === `action:${action.id}`}
                  icon={ACTION_ICONS[action.id] ?? Settings}
                  shortcut={formatAccelerator(effectiveKeys(action, overrides), isMac)}
                  onSelect={() => run(() => runAction(action.id))}
                >
                  <Highlight text={action.label} query={q} />
                </PaletteItem>
              ))}
            </PaletteGroup>
          )}
          {pages.length > 0 && (
            <PaletteGroup heading="Settings">
              {pages.map((page) => (
                <PaletteItem
                  key={page.id}
                  value={`settings:${page.id}`}
                  selected={selected === `settings:${page.id}`}
                  icon={page.icon}
                  onSelect={() => run(() => openSettings(page.id))}
                >
                  Settings › {page.label}
                </PaletteItem>
              ))}
            </PaletteGroup>
          )}
          {q && (
            <PaletteGroup heading="Ask">
              <PaletteItem
                value="ask"
                selected={selected === "ask"}
                icon={MessageCircle}
                shortcut={keys("Mod+Enter")}
                onSelect={() => run(() => askMargin(q))}
              >
                Ask Margin about “{q}”
              </PaletteItem>
            </PaletteGroup>
          )}
        </PaletteList>
        <PaletteFooter
          hints={[
            { keys: [keys("Up")[0] ?? "↑", keys("Down")[0] ?? "↓"], label: "Move" },
            { keys: keys("Enter"), label: "Open" },
            { keys: keys("Mod+Enter"), label: "Ask Margin" },
          ]}
        />
      </PaletteRoot>
    </DialogSurface>
  );
}
