/**
 * The command palette (⌘K): notes, transcript matches, conversations, tasks, actions from the
 * keybind registry and settings pages, on strong glass over a scrim. ⌘↩ hands the query to Cilica.
 *
 * What is typed is read as a search (`parseSearch`): dates, times, people, categories, tags,
 * links, meeting details, import sources and Trash become chips under the input, and the rest
 * are words to find. "Search the words instead" turns the reading off for this query.
 */
import {
  Activity,
  AudioLines,
  Calendar,
  CalendarClock,
  CircleMinus,
  Clock,
  FileText,
  Folder,
  FolderPlus,
  Hash,
  History,
  Import,
  Keyboard,
  Link2,
  ListChecks,
  type LucideIcon,
  MessageCircle,
  MessagesSquare,
  Mic,
  NotebookPen,
  PanelLeft,
  PanelRight,
  Plus,
  Quote,
  Search,
  SearchCheck,
  Settings,
  SquarePen,
  TextQuote,
  Timer,
  Trash2,
  User,
  Users,
  Video,
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
import { navigate, openNote, openSettings } from "@/app/router";
import { askCilica, useAssistantStore } from "@/features/assistant/store";
import { useCategories, useNotes } from "@/features/notes/hooks";
import { ALL_SETTINGS_PAGES } from "@/features/settings/pages";
import { useOverlays } from "@/features/shell/overlays";
import { usePanels } from "@/features/shell/panels";
import { useTasks } from "@/features/tasks/hooks";
import { taskStatus } from "@/features/tasks/TasksView";
import type { NoteQuery, SearchHit } from "@/ipc/types";
import { clock, dayAndTime, displayTitle, relativeDate } from "@/lib/format";
import {
  DialogRoot,
  DialogSurface,
  Excerpt,
  Highlight,
  PaletteEmpty,
  PaletteFilters,
  PaletteFooter,
  PaletteGroup,
  PaletteInput,
  PaletteItem,
  PaletteList,
  PaletteNote,
  PalettePhrase,
  PaletteRoot,
} from "@/ui";
import { useFindNotes, useSearchThreads, useSearchVocabulary } from "./hooks";
import { saidBy, useNearWords, whereSaid } from "./near";
import { type FilterKind, parseSearch, withoutFilter } from "./query";
import { suggestFilters } from "./suggest";

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

const FILTER_ICONS: Record<FilterKind, LucideIcon> = {
  date: Calendar,
  time: Clock,
  kind: Mic,
  person: User,
  category: Folder,
  tag: Hash,
  link: Link2,
  app: Video,
  duration: Timer,
  recording: AudioLines,
  quotes: Quote,
  source: Import,
  trash: Trash2,
  phrase: TextQuote,
  exclude: CircleMinus,
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
  /** Read the query as plain words, without filters. */
  const [literalFor, setLiteralFor] = useState<string | null>(null);
  const [selected, setSelected] = useState("");
  const input = useRef<HTMLInputElement>(null);
  const overrides = useKeybindStore((s) => s.overrides);
  const { data: notes = [] } = useNotes();
  const { data: categories = [] } = useCategories();
  const { data: tasks = [] } = useTasks();
  const vocabulary = useSearchVocabulary();
  const q = query.trim();

  // ── Reading the query ──
  const parsed = useMemo(() => parseSearch(q, vocabulary), [q, vocabulary]);
  // Only for the query it was chosen for: typing more reads filters again.
  const literal = literalFor !== null && literalFor === q;
  const reading = !literal && parsed.filters.length > 0 ? parsed : null;
  const details = reading?.hasDetails ?? false;
  /** The words to find, without the filters. */
  const words = reading ? reading.text : q;
  const noteQuery: NoteQuery | null = useMemo(() => {
    if (!q) return null;
    if (!reading) return { text: q, trash: "include" };
    // Trash results get their own group, so search it alongside unless only Trash was asked.
    return { ...reading.query, trash: reading.query.trash === "only" ? "only" : "include" };
  }, [q, reading]);
  const suggestions = useMemo(
    () => (literal ? [] : suggestFilters(query, parsed, vocabulary)),
    [literal, query, parsed, vocabulary],
  );

  // ── Results ──
  const search = useFindNotes(noteQuery, 40);
  const hits = search.data ?? [];
  const liveHits = hits.filter((h) => !h.note.deleted_at);
  const trashHits = hits
    .filter((h) => h.note.deleted_at)
    .slice(0, reading?.query.trash === "only" ? 12 : 3);
  const threads = useSearchThreads(details ? "" : words);
  const categoryName = (id: string | null) => categories.find((c) => c.id === id)?.name ?? "Notes";

  const noteResults = useMemo(() => {
    if (details) return [];
    if (!q) return [...notes].sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, 6);
    const titleWords = [words, ...(reading?.query.phrases ?? [])].join(" ");
    if (!titleWords.trim()) return [];
    return notes.filter((n) => matches(displayTitle(n.title), titleWords)).slice(0, 8);
  }, [notes, q, words, details, reading]);

  const shownIds = new Set(noteResults.map((n) => n.id));
  const detailHits = details ? liveHits.slice(0, 12) : [];
  const bodyHits =
    q && !details
      ? liveHits.filter((h) => h.source !== "transcript" && !shownIds.has(h.note.id)).slice(0, 5)
      : [];
  const transcriptHits =
    q && !details ? liveHits.filter((h) => h.source === "transcript").slice(0, 4) : [];

  const taskResults =
    words && !details
      ? tasks.filter((t) => matches(`${t.name} ${t.prompt}`, words)).slice(0, 3)
      : [];
  const actions = details
    ? []
    : ACTIONS.filter(
        (a) => "inPalette" in a && a.inPalette && canRunAction(a.id) && (!q || matches(a.label, q)),
      );
  const pages = details
    ? []
    : ALL_SETTINGS_PAGES.filter(
        (p) => !q || matches(`Settings ${p.label} ${p.keywords.join(" ")}`, q),
      );

  // Nothing anywhere: say so, and offer what was said that comes closest.
  const settled = search.isSuccess && !search.isPlaceholderData && !search.isFetching;
  const nothing =
    q.length > 0 &&
    settled &&
    noteResults.length +
      hits.length +
      threads.data.length +
      taskResults.length +
      actions.length +
      pages.length +
      suggestions.length ===
      0;
  const { data: near = [] } = useNearWords(words, nothing && !details && words.length > 0);

  const run = (fn: () => void) => {
    onClose();
    // Let the dialog close and return focus before the action moves it.
    requestAnimationFrame(fn);
  };

  useEffect(() => {
    input.current?.focus();
  }, []);

  const keys = (k: string) => formatAccelerator(k, isMac);

  const openHit = (hit: SearchHit) =>
    run(() =>
      hit.source === "transcript"
        ? openNote(hit.note.id, { panel: "transcript", atMs: hit.at_ms ?? undefined })
        : openNote(hit.note.id),
    );

  const hitHint = (hit: SearchHit) => {
    if (hit.matched_at) return dayAndTime(hit.matched_at);
    if (hit.source === "transcript" && hit.at_ms !== null) {
      return <span className="nt-mono">{clock(hit.at_ms)}</span>;
    }
    return `${categoryName(hit.note.category_id)} · ${relativeDate(hit.note.updated_at)}`;
  };

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
            run(() => askCilica(q));
          }
        }}
      >
        <PaletteInput
          ref={input}
          icon={Search}
          value={query}
          onValueChange={setQuery}
          placeholder="Search notes, dates, people, #tags, or run a command"
          escKeys={keys("Escape")}
        />
        <PaletteFilters
          filters={(reading?.filters ?? []).map((f, i) => ({
            key: String(i),
            label: f.label,
            icon: FILTER_ICONS[f.kind],
          }))}
          onRemove={(key) => {
            const filter = reading?.filters[Number(key)];
            if (filter) setQuery(withoutFilter(query, filter));
            input.current?.focus();
          }}
          action={
            reading?.readFromWords
              ? { label: "Search the words instead", onClick: () => setLiteralFor(q) }
              : undefined
          }
        />
        <PaletteList>
          <PaletteEmpty>No results for “{q}”.</PaletteEmpty>
          {nothing && (
            <PaletteNote>
              {details
                ? "No notes match all of these. Remove a filter to see more."
                : `Nobody has said “${q}” in a meeting, and no note mentions it.`}
            </PaletteNote>
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
          {suggestions.length > 0 && (
            <PaletteGroup heading="Filters">
              {suggestions.map((s) => (
                <PaletteItem
                  key={s.key}
                  value={`filter:${s.key}`}
                  selected={selected === `filter:${s.key}`}
                  icon={FILTER_ICONS[s.kind]}
                  hint="Filter"
                  onSelect={() => {
                    setQuery(s.input);
                    input.current?.focus();
                  }}
                >
                  {s.label}
                </PaletteItem>
              ))}
            </PaletteGroup>
          )}
          {detailHits.length > 0 && (
            <PaletteGroup heading="Notes">
              {detailHits.map((hit) => (
                <PaletteItem
                  key={hit.note.id}
                  value={`found:${hit.note.id}`}
                  selected={selected === `found:${hit.note.id}`}
                  icon={hit.note.kind === "meeting" ? Mic : FileText}
                  hint={hitHint(hit)}
                  onSelect={() => openHit(hit)}
                  sub={
                    hit.source === "body" || hit.source === "transcript" ? (
                      <Excerpt excerpt={hit.excerpt} />
                    ) : undefined
                  }
                >
                  <Highlight text={displayTitle(hit.note.title)} query={words} />
                </PaletteItem>
              ))}
            </PaletteGroup>
          )}
          {noteResults.length + bodyHits.length > 0 && (
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
                  <Highlight text={displayTitle(note.title)} query={words} />
                </PaletteItem>
              ))}
              {bodyHits.map((hit) => (
                <PaletteItem
                  key={`body:${hit.note.id}`}
                  value={`body:${hit.note.id}`}
                  selected={selected === `body:${hit.note.id}`}
                  icon={NotebookPen}
                  hint={categoryName(hit.note.category_id)}
                  onSelect={() => openHit(hit)}
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
                    hint={hitHint(hit)}
                    onSelect={() => openHit(hit)}
                  >
                    <Excerpt excerpt={hit.excerpt} />
                  </PaletteItem>
                );
              })}
            </PaletteGroup>
          )}
          {threads.data.length > 0 && (
            <PaletteGroup heading="Conversations">
              {threads.data.map((hit) => (
                <PaletteItem
                  key={hit.thread.id}
                  value={`thread:${hit.thread.id}`}
                  selected={selected === `thread:${hit.thread.id}`}
                  icon={MessagesSquare}
                  hint={relativeDate(hit.thread.updated_at)}
                  onSelect={() =>
                    run(() => {
                      useAssistantStore.getState().setThreadId(hit.thread.id);
                      usePanels.getState().setAssistantOpen(true);
                    })
                  }
                  sub={<Excerpt excerpt={hit.excerpt} />}
                >
                  {hit.thread.title || "Conversation"}
                </PaletteItem>
              ))}
            </PaletteGroup>
          )}
          {taskResults.length > 0 && (
            <PaletteGroup heading="Tasks">
              {taskResults.map((task) => (
                <PaletteItem
                  key={task.id}
                  value={`task:${task.id}`}
                  selected={selected === `task:${task.id}`}
                  icon={ListChecks}
                  hint={
                    task.last_run
                      ? `${taskStatus(task).label} · ${relativeDate(task.last_run.started_at)}`
                      : taskStatus(task).label
                  }
                  onSelect={() =>
                    run(() =>
                      navigate(
                        task.last_run
                          ? { name: "tasks", taskRunId: task.last_run.id }
                          : { name: "tasks" },
                      ),
                    )
                  }
                >
                  <Highlight text={task.name} query={words} />
                </PaletteItem>
              ))}
            </PaletteGroup>
          )}
          {trashHits.length > 0 && (
            <PaletteGroup heading="In Trash">
              {trashHits.map((hit) => (
                <PaletteItem
                  key={`trash:${hit.note.id}`}
                  value={`trash:${hit.note.id}`}
                  selected={selected === `trash:${hit.note.id}`}
                  icon={Trash2}
                  hint={
                    hit.matched_at
                      ? dayAndTime(hit.matched_at)
                      : `Deleted ${relativeDate(hit.note.deleted_at ?? hit.note.updated_at)}`
                  }
                  onSelect={() => run(() => navigate({ name: "trash" }))}
                >
                  <Highlight text={displayTitle(hit.note.title)} query={words} />
                </PaletteItem>
              ))}
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
                onSelect={() => run(() => askCilica(q))}
              >
                Ask Cilica about “{q}”
              </PaletteItem>
            </PaletteGroup>
          )}
        </PaletteList>
        <PaletteFooter
          hints={[
            { keys: [keys("Up")[0] ?? "↑", keys("Down")[0] ?? "↓"], label: "Move" },
            { keys: keys("Enter"), label: "Open" },
            { keys: keys("Mod+Enter"), label: "Ask Cilica" },
          ]}
        />
      </PaletteRoot>
    </DialogSurface>
  );
}
