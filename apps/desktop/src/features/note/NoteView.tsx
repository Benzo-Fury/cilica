/**
 * The note view: editable title, Notes ⇄ Transcript (⌘E), the Markdown editor, find (⌘F),
 * history, and the overflow menu (export, copy, history, move, Trash).
 */

import {
  AudioLines,
  Calendar,
  Clock,
  Copy,
  Download,
  Ellipsis,
  Folder,
  FolderInput,
  History,
  Inbox,
  NotebookPen,
  Trash2,
  X,
} from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useBackendEvent } from "@/app/backendEvents";
import { useAction, useShortcut } from "@/app/keybinds";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { useRouter } from "@/app/router";
import { Connections } from "@/features/links/Connections";
import {
  useCategories,
  useDiscardEmptyNote,
  useMoveNote,
  useNote,
  useRenameNote,
  useTrashNote,
} from "@/features/notes/hooks";
import { isLive, useRecordingState } from "@/features/recorder/hooks";
import { RecordButton } from "@/features/recorder/RecordButton";
import { CanvasBand } from "@/features/shell/CanvasBand";
import { useTranscript } from "@/features/transcript/hooks";
import { TranscriptPanel } from "@/features/transcript/TranscriptPanel";
import type { AppError, Note, NotePanel } from "@/ipc/types";
import { dayAndTime, displayTitle, duration } from "@/lib/format";
import {
  AvatarStack,
  Button,
  Chip,
  ChipButton,
  Crumb,
  EmptyState,
  Icon,
  IconButton,
  Menu,
  MenuContent,
  MenuItem,
  MenuSeparator,
  MenuSub,
  MenuTrigger,
  PageColumn,
  RecDot,
  ScrollArea,
  SegmentedControl,
  Shimmer,
  Skeleton,
  SkeletonText,
  TitleInput,
  ViewLayer,
} from "@/ui";
import { useBandSlot } from "./bandSlot";
import { FindBar } from "./FindBar";
import { HistoryView } from "./HistoryView";
import { NoteEditor, type NoteEditorHandle } from "./NoteEditor";
import { useNoteActions } from "./useNoteActions";
import { useNoteViewState } from "./viewState";

const PANELS = [
  { value: "notes", label: "Notes", icon: NotebookPen },
  { value: "transcript", label: "Transcript", icon: AudioLines },
] as const;

export function NoteView({ noteId }: { noteId: string }) {
  const { data: note, isLoading, isError, error } = useNote(noteId);
  const navigate = useRouter((s) => s.navigate);
  const missing = isError && (error as unknown as AppError | null)?.code === "not_found";
  // A remembered note that no longer exists (deleted, fresh install): go home quietly.
  useEffect(() => {
    if (missing) navigate({ name: "home" });
  }, [missing, navigate]);
  if (isLoading || missing) return <NoteSkeleton />;
  if (isError || !note) {
    return (
      <ViewLayer>
        <CanvasBand />
        <ScrollArea>
          <EmptyState art="search" title="This note is not here any more">
            It may have been moved to Trash or deleted. Pick another note in the sidebar.
          </EmptyState>
        </ScrollArea>
      </ViewLayer>
    );
  }
  return <LoadedNote note={note} />;
}

function NoteSkeleton() {
  return (
    <ViewLayer>
      <CanvasBand />
      <ScrollArea>
        <PageColumn>
          <div className="nt-note">
            <Skeleton size="title" />
            <div className="nt-note-meta nt-skel-gap" />
            <SkeletonText lines={5} />
          </div>
        </PageColumn>
      </ScrollArea>
    </ViewLayer>
  );
}

function LoadedNote({ note }: { note: Note }) {
  const setBandSlot = useBandSlot((s) => s.setEl);
  const reduced = useReducedMotion();
  const routePanel = useRouter((s) => (s.view.name === "note" ? s.view.panel : undefined));
  const atMs = useRouter((s) => (s.view.name === "note" ? s.view.atMs : undefined));
  const recording = useRecordingState();
  const isRecordingHere = isLive(recording) && recording.note_id === note.id;
  const [panel, setPanel] = useState<NotePanel>(
    routePanel ?? (isRecordingHere ? "transcript" : "notes"),
  );
  const [mode, setMode] = useState<"view" | "history">("view");
  const [find, setFind] = useState<{ open: boolean; tick: number }>({ open: false, tick: 0 });
  const [streaming, setStreaming] = useState(false);
  const [title, setTitle] = useState(note.title);
  const editorRef = useRef<NoteEditorHandle>(null);
  const titleRef = useRef<HTMLTextAreaElement>(null);
  const { data: categories = [] } = useCategories();
  const rename = useRenameNote();
  const move = useMoveNote();
  const trash = useTrashNote();
  const { data: transcript } = useTranscript(note.id, note.kind === "meeting");
  const category = categories.find((c) => c.id === note.category_id);
  const keys = {
    panel: useShortcut("note.togglePanel"),
    trash: useShortcut("note.trash"),
    find: useShortcut("note.find"),
    history: useShortcut("note.history"),
  };

  // Keep the title in sync with renames from elsewhere (sidebar, Margin), unless editing it.
  useEffect(() => {
    if (document.activeElement !== titleRef.current) setTitle(note.title);
  }, [note.title]);

  // A brand-new note starts in its title.
  // biome-ignore lint/correctness/useExhaustiveDependencies: only on first mount of this note
  useEffect(() => {
    if (!note.title && !note.body_md) titleRef.current?.focus();
  }, []);

  // A note opened empty and left empty is not kept. Layout cleanup runs while the editor is
  // still attached, so what was typed but not yet saved counts; without the editor, keep it.
  const discardEmpty = useDiscardEmptyNote();
  const blankAtOpen = useRef(note.kind === "manual" && !note.title.trim() && !note.body_md.trim());
  const titleNow = useRef(title);
  titleNow.current = title;
  useLayoutEffect(() => {
    const id = note.id;
    return () => {
      const view = useRouter.getState().view;
      if (view.name === "note" && view.noteId === id) return; // still here (dev double-mount)
      const body = editorRef.current?.markdown();
      if (!blankAtOpen.current || body === undefined) return;
      if (titleNow.current.trim() || body.trim()) return;
      discardEmpty(id);
    };
  }, [note.id, discardEmpty]);

  const flush = useCallback(() => editorRef.current?.flush() ?? Promise.resolve(), []);
  const markdown = useCallback(() => editorRef.current?.markdown() ?? note.body_md, [note.body_md]);
  const { exportMarkdown, copyMarkdown } = useNoteActions(note.id, note.title, markdown, flush);

  const commitTitle = (value: string) => {
    const next = value.trim();
    if (next !== note.title.trim()) rename.mutate({ noteId: note.id, title: next });
  };

  const switchPanel = (next: NotePanel) => {
    if (next === panel) return;
    if (panel === "notes") void flush();
    setFind((f) => ({ ...f, open: false }));
    setPanel(next);
  };

  // The assistant sees which panel is showing.
  useEffect(() => {
    useNoteViewState.getState().setPanel(note.id, panel);
  }, [note.id, panel]);
  useEffect(() => () => useNoteViewState.getState().clear(note.id), [note.id]);

  // When Margin starts writing the meeting summary, the view slides to Notes to show it.
  useBackendEvent("note_body_stream", (event) => {
    if (event.note_id === note.id && !event.done && panel !== "notes") {
      setMode("view");
      setPanel("notes");
    }
  });

  useAction("note.togglePanel", () => switchPanel(panel === "notes" ? "transcript" : "notes"));
  useAction("note.trash", () => trash.mutate(note.id));
  // On the Transcript side, the transcript's own find takes ⌘F.
  useAction(
    "note.find",
    () => {
      if (mode !== "view") setMode("view");
      setFind((f) => ({ open: true, tick: f.tick + 1 }));
    },
    panel === "notes" || mode === "history",
  );
  useAction("note.history", () => {
    void flush();
    setMode((m) => (m === "history" ? "view" : "history"));
  });

  const direction = panel === "transcript" ? 1 : -1;
  const slide = {
    initial: { opacity: 0, x: reduced ? 0 : 8 * direction },
    animate: { opacity: 1, x: 0, transition: reduced ? reducedTransition : springs.gentle },
    exit: { opacity: 0, x: reduced ? 0 : -8 * direction, transition: tweens.exit },
  };

  const people = (transcript?.speakers ?? []).map((s) => ({ name: s.label, color: s.color_index }));

  return (
    <ViewLayer>
      <CanvasBand
        start={
          <Crumb
            parts={[
              <span key="cat" className="nt-crumb-cat">
                <Icon icon={category ? Folder : Inbox} size="sm" />
                {category?.name ?? "Notes"}
              </span>,
              displayTitle(title),
            ]}
          />
        }
        center={
          mode === "history" ? (
            <span className="nt-band-title">History</span>
          ) : (
            <>
              <SegmentedControl
                label="Note panel"
                options={PANELS}
                value={panel}
                onChange={switchPanel}
                segmentWidth="wide"
              />
              <RecordButton />
              <span ref={setBandSlot} className="nt-band-slot" />
            </>
          )
        }
        end={
          mode === "history" ? (
            <Button
              variant="secondary"
              size="sm"
              icon={X}
              iconMotion="x"
              onClick={() => setMode("view")}
            >
              Close history
            </Button>
          ) : (
            <Menu>
              <MenuTrigger asChild>
                <IconButton icon={Ellipsis} label="More" />
              </MenuTrigger>
              <MenuContent>
                <MenuItem icon={Download} iconMotion="up" onSelect={() => void exportMarkdown()}>
                  Export as Markdown…
                </MenuItem>
                <MenuItem icon={Copy} iconMotion="copy" onSelect={() => void copyMarkdown()}>
                  Copy as Markdown
                </MenuItem>
                <MenuItem
                  icon={History}
                  iconMotion="clock"
                  shortcut={keys.history}
                  onSelect={() => {
                    void flush();
                    setMode("history");
                  }}
                >
                  Show history
                </MenuItem>
                <MenuSub label="Move to" icon={FolderInput} iconMotion="arrow">
                  <MenuItem
                    icon={Inbox}
                    disabled={!note.category_id}
                    onSelect={() =>
                      move.mutate({
                        noteId: note.id,
                        categoryId: null,
                        position: null,
                        announce: "Notes",
                      })
                    }
                  >
                    Notes
                  </MenuItem>
                  {categories.map((c) => (
                    <MenuItem
                      key={c.id}
                      icon={Folder}
                      iconMotion="folder"
                      disabled={note.category_id === c.id}
                      onSelect={() =>
                        move.mutate({
                          noteId: note.id,
                          categoryId: c.id,
                          position: null,
                          announce: c.name,
                        })
                      }
                    >
                      {c.name}
                    </MenuItem>
                  ))}
                </MenuSub>
                <MenuSeparator />
                <MenuItem
                  icon={Trash2}
                  iconMotion="trash"
                  danger
                  shortcut={keys.trash}
                  onSelect={() => trash.mutate(note.id)}
                >
                  Move to Trash
                </MenuItem>
              </MenuContent>
            </Menu>
          )
        }
      />
      <div className="nt-note-body">
        <AnimatePresence>
          {find.open && mode === "view" && panel === "notes" && (
            <FindBar
              editor={editorRef.current?.editor ?? null}
              focusTick={find.tick}
              onClose={() => setFind((f) => ({ ...f, open: false }))}
            />
          )}
        </AnimatePresence>
        {mode === "history" ? (
          <motion.div className="nt-view" {...slide}>
            <HistoryView
              note={note}
              currentBody={markdown()}
              onRestored={(md) => editorRef.current?.replace(md)}
              onClose={() => setMode("view")}
            />
          </motion.div>
        ) : null}
        <AnimatePresence initial={false} mode="popLayout">
          <motion.div
            key={panel}
            className="nt-view"
            {...slide}
            style={{ visibility: mode === "history" ? "hidden" : undefined }}
            aria-hidden={mode === "history"}
          >
            {panel === "notes" ? (
              <ScrollArea>
                <PageColumn>
                  <article className="nt-note" data-streaming={streaming || undefined}>
                    <TitleInput
                      ref={titleRef}
                      value={title}
                      onChange={setTitle}
                      onCommit={commitTitle}
                      onExit={() => editorRef.current?.focusStart()}
                    />
                    <div className="nt-note-meta">
                      <Chip icon={Calendar}>{dayAndTime(note.started_at ?? note.created_at)}</Chip>
                      {note.started_at && note.ended_at && (
                        <Chip icon={Clock}>
                          {duration(
                            new Date(note.ended_at).getTime() - new Date(note.started_at).getTime(),
                          )}
                        </Chip>
                      )}
                      {isRecordingHere && (
                        <Chip tone="rec" leading={<RecDot size="sm" />}>
                          Recording
                        </Chip>
                      )}
                      {people.length > 0 && (
                        <Chip leading={<AvatarStack people={people} />}>
                          {people.length === 1 ? "1 person" : `${people.length} people`}
                        </Chip>
                      )}
                      <Menu>
                        <MenuTrigger asChild>
                          <ChipButton
                            tone={category ? "accent" : "neutral"}
                            icon={category ? Folder : FolderInput}
                          >
                            {category?.name ?? "Add to category"}
                          </ChipButton>
                        </MenuTrigger>
                        <MenuContent align="start">
                          <MenuItem
                            icon={Inbox}
                            disabled={!note.category_id}
                            onSelect={() =>
                              move.mutate({
                                noteId: note.id,
                                categoryId: null,
                                position: null,
                                announce: "Notes",
                              })
                            }
                          >
                            Notes
                          </MenuItem>
                          {categories.map((c) => (
                            <MenuItem
                              key={c.id}
                              icon={Folder}
                              iconMotion="folder"
                              disabled={note.category_id === c.id}
                              onSelect={() =>
                                move.mutate({
                                  noteId: note.id,
                                  categoryId: c.id,
                                  position: null,
                                  announce: c.name,
                                })
                              }
                            >
                              {c.name}
                            </MenuItem>
                          ))}
                        </MenuContent>
                      </Menu>
                    </div>
                    {streaming && (
                      <div className="nt-streaming-line">
                        <Shimmer>Writing notes from the meeting…</Shimmer>
                      </div>
                    )}
                    <NoteEditor ref={editorRef} note={note} onStreamingChange={setStreaming} />
                  </article>
                  {!streaming && (note.title.trim() || note.body_md.trim()) && (
                    <Connections noteId={note.id} categoryId={note.category_id} />
                  )}
                </PageColumn>
              </ScrollArea>
            ) : (
              <TranscriptPanel noteId={note.id} isMeeting={note.kind === "meeting"} atMs={atMs} />
            )}
          </motion.div>
        </AnimatePresence>
      </div>
    </ViewLayer>
  );
}
