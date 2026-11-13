/**
 * The Transcript side of a note: speaker turns with avatar, name and timecode; live mode with
 * partial segments refining in place, auto-scroll that pauses when you scroll up and a "Jump to
 * live" chip; speaker naming and voiceprint suggestions; playback with seek and the current
 * segment highlighted; find (⌘F); and search hits (`atMs`) scrolled to and washed.
 * Turns are windowed, so an hour-long meeting scrolls as smoothly as a short one.
 */
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  AudioLines,
  ChevronDown,
  ChevronUp,
  Mic,
  Search,
  Settings2,
  UsersRound,
  X,
} from "lucide-react";
import { AnimatePresence } from "motion/react";
import {
  type KeyboardEvent,
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { formatAccelerator, ShortcutTip, useAction, useShortcut } from "@/app/keybinds";
import { isMac } from "@/app/platform";
import { openSettings } from "@/app/router";
import { useBandSlot } from "@/features/note/bandSlot";
import { useNote } from "@/features/notes/hooks";
import { usePeople } from "@/features/people/hooks";
import { isLive, useRecordingState, useToggleRecording } from "@/features/recorder/hooks";
import { useSettings } from "@/features/settings/hooks";
import { useSwearMarks } from "@/features/swearjar/hooks";
import type { Person, Quote, Segment, Speaker, SpeakerSuggestion } from "@/ipc/types";
import { clock, plural } from "@/lib/format";
import {
  Avatar,
  AvatarStack,
  Button,
  Chip,
  EmptyState,
  Field,
  IconButton,
  InlinePrompt,
  JumpChip,
  Kbd,
  PageColumn,
  ScrollArea,
  SegmentText,
  Shimmer,
  SkeletonText,
  TimeButton,
  Utterance,
} from "@/ui";
import { BandPlayer, type RecordingDetails } from "./BandPlayer";
import {
  useAudioSource,
  useDeclinedSuggestions,
  useSpeakerActions,
  useSpeakerSuggestions,
  useTranscript,
} from "./hooks";
import {
  type FindMatch,
  findMatches,
  groupTurns,
  type SpeakerLook,
  segmentAt,
  speakerLook,
  speakersPending,
  type Turn,
  turnIndexAt,
} from "./model";
import { SpeakerMenu } from "./SpeakerMenu";
import { useAudioPlayer } from "./useAudioPlayer";

export interface TranscriptPanelProps {
  noteId: string;
  isMeeting: boolean;
  atMs?: number;
}

export function TranscriptPanel({ noteId, isMeeting, atMs }: TranscriptPanelProps) {
  const { data: transcript, isLoading } = useTranscript(noteId);
  const recording = useRecordingState();
  const live = isLive(recording) && recording.note_id === noteId;
  const toggle = useToggleRecording();
  const { data: settings } = useSettings();
  const hideSpeakers = speakersPending(recording, noteId);

  if (isLoading || !transcript) {
    return (
      <ScrollArea>
        <PageColumn>
          <SkeletonText lines={6} />
        </PageColumn>
      </ScrollArea>
    );
  }
  if (transcript.segments.length === 0) {
    return (
      <ScrollArea>
        {live ? (
          <EmptyState art="transcript" title="Preparing transcription" className="nt-empty-page">
            <Shimmer>Recording has started. Words appear here within a few seconds.</Shimmer>
          </EmptyState>
        ) : isMeeting ? (
          <EmptyState
            art="transcript"
            title="No speech in this recording"
            className="nt-empty-page"
            actions={
              <Button
                variant="secondary"
                icon={Settings2}
                iconMotion="gear"
                onClick={() => openSettings("recording")}
              >
                Check recording settings
              </Button>
            }
          >
            Cilica heard nobody speak. Check that the right microphone is chosen and that Cilica can
            hear call audio before your next meeting.
          </EmptyState>
        ) : (
          <EmptyState
            art="transcript"
            title="No transcript for this note"
            className="nt-empty-page"
            tip={<ShortcutTip action="recording.toggle">Start recording from anywhere</ShortcutTip>}
            actions={
              <Button
                variant="primary"
                icon={Mic}
                iconMotion="mic"
                onClick={() => void toggle("manual", noteId)}
              >
                Record into this note
              </Button>
            }
          >
            Record a meeting into this note and every word appears here, labelled by speaker. What
            you have written stays and is woven into the summary.
          </EmptyState>
        )}
      </ScrollArea>
    );
  }
  return (
    <TranscriptBody
      key={noteId}
      noteId={noteId}
      speakers={transcript.speakers}
      segments={transcript.segments}
      hasAudio={transcript.has_audio}
      timed={transcript.timed}
      quotes={transcript.quotes}
      durationMs={
        transcript.duration_ms ?? transcript.segments[transcript.segments.length - 1]?.end_ms ?? 0
      }
      live={live}
      idle={recording.phase === "idle"}
      hideSpeakers={hideSpeakers}
      speakersLater={settings?.transcription.identify_speakers_after_meeting ?? true}
      atMs={atMs}
    />
  );
}

const NEAR_BOTTOM = 64;

function TranscriptBody({
  noteId,
  speakers,
  segments,
  hasAudio,
  timed,
  quotes,
  durationMs,
  live,
  idle,
  hideSpeakers,
  speakersLater,
  atMs,
}: {
  noteId: string;
  speakers: Speaker[];
  segments: Segment[];
  hasAudio: boolean;
  /** False for imported transcripts that had no times: their segment times only keep the order. */
  timed: boolean;
  quotes: Quote[];
  durationMs: number;
  live: boolean;
  idle: boolean;
  /** The engine does not label speakers live: leave them out until the meeting is finished. */
  hideSpeakers: boolean;
  /** Speakers will be identified once the meeting is finished (Settings › Transcription). */
  speakersLater: boolean;
  atMs?: number;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLOListElement>(null);
  const findRef = useRef<HTMLInputElement>(null);
  const [scrollMargin, setScrollMargin] = useState(0);
  const [following, setFollowing] = useState(true);
  const [hitTurn, setHitTurn] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [current, setCurrent] = useState(0);
  const { data: people = [] } = usePeople();
  const turns = useMemo(() => groupTurns(segments), [segments]);
  const speakerById = useMemo(() => new Map(speakers.map((s) => [s.id, s])), [speakers]);
  const findKeys = useShortcut("note.find");

  // ── Suggestions ("Is Person 2 Sam?") ──
  const hasUnnamed = speakers.some((s) => speakerLook(s, people).unnamed);
  const { data: suggestions = [] } = useSpeakerSuggestions(noteId, !live && hasUnnamed);
  const declined = useDeclinedSuggestions((s) => s.declined);
  const suggestionFor = useMemo(() => {
    const map = new Map<string, SpeakerSuggestion>();
    for (const s of suggestions) {
      if (!declined.has(`${s.speaker_id}:${s.person_id}`) && !map.has(s.speaker_id))
        map.set(s.speaker_id, s);
    }
    return map;
  }, [suggestions, declined]);
  const firstTurnOf = useMemo(() => {
    const seen = new Map<string, string>();
    for (const t of turns) if (t.speakerId && !seen.has(t.speakerId)) seen.set(t.speakerId, t.id);
    return seen;
  }, [turns]);

  // ── Playback ──
  const playable = hasAudio && !live && idle;
  const { data: src } = useAudioSource(noteId, playable);
  const player = useAudioPlayer(playable ? src : null, durationMs);
  const playingSegment =
    playable && src && (player.playing || player.positionMs > 0)
      ? segmentAt(segments, player.positionMs)
      : null;
  const showBar = Boolean(playable && src);
  const bandSlot = useBandSlot((s) => s.el);
  const { data: note } = useNote(noteId);
  const details = useMemo<RecordingDetails>(() => {
    const spoken = new Map<string, number>();
    let total = 0;
    let system = false;
    for (const seg of segments) {
      if (seg.channel !== "mic") system = true;
      if (!seg.speaker_id) continue;
      const length = Math.max(0, seg.end_ms - seg.start_ms);
      spoken.set(seg.speaker_id, (spoken.get(seg.speaker_id) ?? 0) + length);
      total += length;
    }
    return {
      recordedAt: note?.started_at ?? note?.created_at ?? null,
      durationMs,
      speakers: speakers
        .map((s) => {
          const look = speakerLook(s, people);
          return {
            name: look.name,
            color: look.color,
            share: total > 0 ? (spoken.get(s.id) ?? 0) / total : 0,
          };
        })
        .filter((s) => s.share > 0),
      source: system ? `Microphone and ${note?.meeting_app ?? "call audio"}` : "Microphone",
    };
  }, [
    segments,
    speakers,
    people,
    durationMs,
    note?.started_at,
    note?.created_at,
    note?.meeting_app,
  ]);

  // ── Find ──
  const matches = useMemo(() => findMatches(turns, query), [turns, query]);
  const matchesBySegment = useMemo(() => {
    const map = new Map<string, { start: number; end: number; current: boolean }[]>();
    matches.forEach((m: FindMatch, i) => {
      const list = map.get(m.segmentId) ?? [];
      list.push({ start: m.start, end: m.end, current: i === current });
      map.set(m.segmentId, list);
    });
    return map;
  }, [matches, current]);
  useAction("note.find", () => {
    findRef.current?.focus();
    findRef.current?.select();
  });

  // ── Windowing ──
  const virtualizer = useVirtualizer({
    count: turns.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 76,
    overscan: 6,
    scrollMargin,
    getItemKey: (index) => turns[index]?.id ?? index,
  });
  useLayoutEffect(() => {
    if (listRef.current) setScrollMargin(listRef.current.offsetTop);
  }, []);

  const scrollToTurn = useCallback(
    (index: number, align: "center" | "auto" = "center") => {
      if (index < 0) return;
      setFollowing(false);
      virtualizer.scrollToIndex(index, { align });
    },
    [virtualizer],
  );

  // Live: follow the newest words until the person scrolls up.
  const lastText = segments[segments.length - 1]?.text;
  // biome-ignore lint/correctness/useExhaustiveDependencies: follow every new or refined segment
  useEffect(() => {
    if (!live || !following) return;
    const el = scrollRef.current;
    if (!el) return;
    const frame = requestAnimationFrame(() => {
      const distance = el.scrollHeight - el.clientHeight - el.scrollTop;
      el.scrollTo({ top: el.scrollHeight, behavior: distance > 600 ? "auto" : "smooth" });
    });
    return () => cancelAnimationFrame(frame);
  }, [live, following, segments.length, lastText]);

  // A search hit: scroll to it, wash it, and park the playhead there.
  // biome-ignore lint/correctness/useExhaustiveDependencies: once per hit
  useEffect(() => {
    if (atMs === undefined) return;
    const index = turnIndexAt(turns, atMs);
    const turn = turns[index];
    if (!turn) return;
    const t = setTimeout(() => scrollToTurn(index), 60);
    setHitTurn(turn.id);
    const clear = setTimeout(() => setHitTurn(null), 2600);
    return () => {
      clearTimeout(t);
      clearTimeout(clear);
    };
  }, [atMs]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: seek once the audio is ready
  useEffect(() => {
    if (atMs !== undefined && src) player.seek(atMs);
  }, [src]);

  // Playing: keep the current turn on screen.
  const playingTurn = playingSegment
    ? turns.findIndex((t) => t.segments.includes(playingSegment))
    : -1;
  useEffect(() => {
    if (player.playing && playingTurn >= 0)
      virtualizer.scrollToIndex(playingTurn, { align: "auto" });
  }, [player.playing, playingTurn, virtualizer]);

  // Find: keep the current match in view.
  useEffect(() => {
    const m = matches[current];
    if (m) scrollToTurn(m.turnIndex);
  }, [matches, current, scrollToTurn]);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    if (el.scrollHeight - el.clientHeight - el.scrollTop < NEAR_BOTTOM) setFollowing(true);
  };
  const pauseFollowing = () => {
    if (live) setFollowing(false);
  };

  const step = (direction: 1 | -1) => {
    if (!matches.length) return;
    setCurrent((c) => (c + direction + matches.length) % matches.length);
  };
  const onFindKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      step(event.shiftKey ? -1 : 1);
    } else if (event.key === "Escape" && query) {
      event.preventDefault();
      event.stopPropagation();
      setQuery("");
    }
  };

  const speakerCount = new Set(turns.map((t) => t.speakerId ?? t.channel)).size;
  const stack = [...new Set(turns.map((t) => t.speakerId))]
    .map((id) => speakerLook(id ? speakerById.get(id) : undefined, people))
    .slice(0, 3)
    .map((l) => ({ name: l.name, color: l.color }));

  const seekTo =
    playable && src
      ? (ms: number) => {
          player.seek(ms);
          if (!player.playing) player.play();
        }
      : undefined;

  return (
    <div className="nt-transcript">
      <ScrollArea
        ref={scrollRef}
        className="nt-transcript-scroll"
        onScroll={onScroll}
        onWheel={(e) => e.deltaY < 0 && pauseFollowing()}
        onPointerDown={(e) => {
          // Grabbing the scrollbar pauses following too.
          if (e.target === e.currentTarget) pauseFollowing();
        }}
        onKeyDown={(e) => ["ArrowUp", "PageUp", "Home"].includes(e.key) && pauseFollowing()}
      >
        <PageColumn className="nt-transcript-page">
          <div className="nt-transcript-head">
            <Field
              ref={findRef}
              icon={Search}
              size="sm"
              className="nt-grow"
              fieldClassName="nt-field-quiet"
              type="search"
              value={query}
              placeholder="Find in transcript"
              aria-label="Find in transcript"
              onChange={(e) => {
                setQuery(e.target.value);
                setCurrent(0);
              }}
              onKeyDown={onFindKey}
              trailing={
                query.trim() ? (
                  <>
                    <span className="nt-findbar-count" aria-live="polite">
                      {matches.length ? `${current + 1} of ${matches.length}` : "No matches"}
                    </span>
                    <IconButton
                      icon={ChevronUp}
                      size="sm"
                      label="Previous match"
                      shortcut={formatAccelerator("Shift+Enter", isMac)}
                      disabled={!matches.length}
                      onClick={() => step(-1)}
                    />
                    <IconButton
                      icon={ChevronDown}
                      size="sm"
                      label="Next match"
                      shortcut={formatAccelerator("Enter", isMac)}
                      disabled={!matches.length}
                      onClick={() => step(1)}
                    />
                    <IconButton
                      icon={X}
                      iconMotion="x"
                      size="sm"
                      label="Clear"
                      onClick={() => setQuery("")}
                    />
                  </>
                ) : (
                  <Kbd keys={findKeys} />
                )
              }
            />
            {hideSpeakers ? (
              speakersLater && <Chip icon={UsersRound}>Speakers appear when it's finished</Chip>
            ) : (
              <Chip leading={<AvatarStack people={stack} />}>
                {plural(speakerCount, "speaker")}
              </Chip>
            )}
            {live && <Chip icon={AudioLines}>Live</Chip>}
          </div>
          <ol
            ref={listRef}
            className="nt-transcript-list"
            style={{ height: virtualizer.getTotalSize() }}
            aria-label="Transcript"
          >
            {virtualizer.getVirtualItems().map((item) => {
              const turn = turns[item.index];
              if (!turn) return null;
              const speaker = turn.speakerId ? speakerById.get(turn.speakerId) : undefined;
              return (
                <li
                  key={item.key}
                  data-index={item.index}
                  ref={virtualizer.measureElement}
                  style={{ transform: `translateY(${item.start - scrollMargin}px)` }}
                >
                  <TurnRow
                    noteId={noteId}
                    turn={turn}
                    speaker={speaker}
                    look={speakerLook(speaker, people, turn.channel)}
                    people={people}
                    live={live}
                    anonymous={hideSpeakers}
                    hit={hitTurn === turn.id}
                    playingId={playingSegment?.id ?? null}
                    marks={matchesBySegment}
                    first={Boolean(turn.speakerId && firstTurnOf.get(turn.speakerId) === turn.id)}
                    suggestion={
                      turn.speakerId && firstTurnOf.get(turn.speakerId) === turn.id
                        ? suggestionFor.get(turn.speakerId)
                        : undefined
                    }
                    timed={timed}
                    onSeek={seekTo}
                  />
                </li>
              );
            })}
          </ol>
          {!live && quotes.length > 0 && (
            <section className="nt-quotes" aria-label="Lines Cilica kept">
              <span className="nt-quotes-label">Lines worth keeping</span>
              {quotes.map((quote) => {
                const index = turnIndexAt(turns, quote.at_ms);
                const turn = turns[index];
                const speaker = turn?.speakerId ? speakerById.get(turn.speakerId) : undefined;
                const look = speakerLook(speaker, people, turn?.channel);
                return (
                  <button
                    key={`${quote.at_ms}-${quote.text}`}
                    type="button"
                    className="nt-quote"
                    title={seekTo ? "Play from here" : "Show in the transcript"}
                    onClick={() => {
                      if (!turn) return;
                      scrollToTurn(index);
                      setHitTurn(turn.id);
                      setTimeout(() => setHitTurn(null), 2600);
                      seekTo?.(quote.at_ms);
                    }}
                  >
                    <span className="nt-quote-text">“{quote.text}”</span>
                    <span className="nt-quote-by">
                      {look.name}
                      <time>{clock(quote.at_ms)}</time>
                    </span>
                  </button>
                );
              })}
            </section>
          )}
        </PageColumn>
      </ScrollArea>
      <AnimatePresence>
        {live && !following && (
          <div className="nt-jump-dock">
            <JumpChip
              onClick={() => {
                setFollowing(true);
                const el = scrollRef.current;
                el?.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
              }}
            >
              Jump to live
            </JumpChip>
          </div>
        )}
      </AnimatePresence>
      {showBar &&
        bandSlot &&
        createPortal(
          <BandPlayer
            playing={player.playing}
            positionMs={player.positionMs}
            durationMs={player.durationMs}
            rate={player.rate}
            onToggle={player.toggle}
            onSeek={player.seek}
            onSkip={player.skip}
            onRate={player.cycleRate}
            details={details}
          />,
          bandSlot,
        )}
    </div>
  );
}

const TurnRow = memo(function TurnRow({
  noteId,
  turn,
  speaker,
  look,
  people,
  live,
  anonymous,
  hit,
  playingId,
  marks,
  suggestion,
  first,
  timed,
  onSeek,
}: {
  noteId: string;
  first: boolean;
  timed: boolean;
  turn: Turn;
  speaker: Speaker | undefined;
  look: SpeakerLook;
  people: readonly Person[];
  live: boolean;
  /** Leave out who is speaking (not known yet). */
  anonymous: boolean;
  hit: boolean;
  playingId: string | null;
  marks: Map<string, { start: number; end: number; current: boolean }[]>;
  suggestion: SpeakerSuggestion | undefined;
  onSeek?: (ms: number) => void;
}) {
  const { link } = useSpeakerActions(noteId);
  const decline = useDeclinedSuggestions((s) => s.decline);
  const swearsIn = useSwearMarks();
  return (
    <Utterance
      hit={hit}
      className={anonymous ? "nt-utt-anon" : undefined}
      avatar={anonymous ? null : <Avatar name={look.name} color={look.color} />}
      name={
        anonymous ? null : speaker ? (
          <SpeakerMenu
            noteId={noteId}
            speaker={speaker}
            look={look}
            people={people}
            showChip={first && !suggestion}
          />
        ) : (
          <span className="nt-utt-name">{look.name}</span>
        )
      }
      time={
        timed && (
          <TimeButton
            label={`Play from ${clock(turn.startMs)}`}
            onSeek={onSeek ? () => onSeek(turn.startMs) : undefined}
          >
            {clock(turn.startMs)}
          </TimeButton>
        )
      }
      extra={
        <AnimatePresence>
          {suggestion && (
            <InlinePrompt
              key={suggestion.person_id}
              actions={
                <>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      link.mutate({
                        speakerId: suggestion.speaker_id,
                        personId: suggestion.person_id,
                      })
                    }
                  >
                    Yes
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => decline(`${suggestion.speaker_id}:${suggestion.person_id}`)}
                  >
                    No
                  </Button>
                </>
              }
            >
              Is {look.name} {suggestion.person_name}?
            </InlinePrompt>
          )}
        </AnimatePresence>
      }
    >
      {turn.segments.map((s) => (
        <SegmentText
          key={s.id}
          text={s.text}
          partial={!s.is_final}
          live={live}
          playing={s.id === playingId}
          marks={marks.get(s.id)}
          swears={swearsIn(s)}
          segmentId={s.id}
          onClick={onSeek ? () => onSeek(s.start_ms) : undefined}
        />
      ))}
    </Utterance>
  );
});
