/**
 * The Dynamic Island window (`?window=island`, DynamicIsland doc and the AppInCall mockup): a
 * compact pill with the breathing dot, a five-bar meter and the timer that expands on hover or
 * click to the meeting title, its status and Stop. True black on Macs with a notch, dark glass
 * elsewhere. It loads only what it needs: no router, query cache or main app.
 */
import { AppWindow, Play } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, isTauri, onBackendEvent } from "@/ipc/client";
import type { AudioLevels, RecordingState } from "@/ipc/types";
import { clock } from "@/lib/format";
import { Island, IslandButton, IslandStop } from "@/ui/Island";
import { LevelMeter } from "@/ui/Level";

const COMPACT = { width: 168, height: 32 } as const;
const EXPANDED = { width: 360, height: 76 } as const;
/** How long the morph back to compact takes before the window shrinks (--spring-bouncy-dur). */
const COLLAPSE_MS = 720;

function setSize(size: { width: number; height: number }) {
  if (isTauri) void api.setIslandSize(size.width, size.height).catch(() => undefined);
}

function useRecording() {
  const [state, setState] = useState<RecordingState | null>(null);
  const [levels, setLevels] = useState<AudioLevels>({ mic: 0, system: 0 });
  const [title, setTitle] = useState<string>("");
  const last = useRef(0);
  useEffect(() => {
    void api.getRecordingState().then(setState, () => undefined);
    return onBackendEvent((event) => {
      if (event.type === "recording_state_changed") setState(event.state);
      if (event.type === "audio_levels") {
        const now = performance.now();
        if (now - last.current < 90) return;
        last.current = now;
        setLevels(event.levels);
      }
    });
  }, []);
  const noteId = state?.note_id ?? null;
  useEffect(() => {
    if (!noteId) return;
    void api.getNote(noteId).then(
      (note) => setTitle(note.title),
      () => setTitle(""),
    );
  }, [noteId]);
  return { state, levels, title };
}

function useNow(active: boolean) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(t);
  }, [active]);
  return now;
}

function IslandApp() {
  const { state, levels, title } = useRecording();
  const [hasNotch, setHasNotch] = useState(true);
  const [expanded, setExpanded] = useState(false);
  const [pinned, setPinned] = useState(false);
  const collapseTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const live = state?.phase === "recording" || state?.phase === "starting";
  const finishing = state?.phase === "stopping" || state?.phase === "summarizing";
  const now = useNow(Boolean(state && state.phase !== "idle"));

  useEffect(() => {
    void api.getIslandInfo().then(
      (info) => setHasNotch(info.has_notch),
      () => undefined,
    );
    setSize(COMPACT);
  }, []);

  const expand = useCallback(() => {
    if (collapseTimer.current) clearTimeout(collapseTimer.current);
    // Grow the window first so the morph is never clipped.
    setSize(EXPANDED);
    setExpanded(true);
  }, []);
  const collapse = useCallback(() => {
    setExpanded(false);
    if (collapseTimer.current) clearTimeout(collapseTimer.current);
    collapseTimer.current = setTimeout(() => setSize(COMPACT), COLLAPSE_MS);
  }, []);

  const started = state?.started_at ? Date.parse(state.started_at) : now;
  const elapsed = clock(Math.max(0, now - started));
  const autoStop = state?.auto_stop_at
    ? Math.max(0, Math.ceil((Date.parse(state.auto_stop_at) - now) / 1000))
    : null;
  const level = Math.max(levels.mic ?? 0, levels.system ?? 0);
  const status = finishing
    ? "Writing notes…"
    : autoStop !== null
      ? `Meeting ended · stops in ${autoStop}s`
      : `Recording · ${elapsed} · Cilica`;

  if (!state || state.phase === "idle") return <div className="nt-island-host" />;

  return (
    <div className="nt-island-host" data-preview={isTauri ? undefined : ""}>
      <Island
        label={`Cilica ${live ? "recording" : "writing notes"}, ${elapsed}. Show controls`}
        expanded={expanded}
        glass={!hasNotch}
        onToggle={() => {
          if (expanded && pinned) {
            setPinned(false);
            collapse();
          } else {
            setPinned(true);
            expand();
          }
        }}
        onPointerEnter={expand}
        onPointerLeave={() => !pinned && collapse()}
        compact={
          <>
            <span className="nt-rec-dot" />
            <LevelMeter level={live ? level : 0.1} bars={5} />
            <span className="nt-mono nt-island-time">{finishing ? "Notes" : elapsed}</span>
          </>
        }
        full={
          <>
            <span className="nt-rec-dot nt-island-dot" />
            <div className="nt-island-text">
              <div className="nt-island-title">{title || "Meeting"}</div>
              <div className="nt-island-status" aria-live="polite">
                {status}
              </div>
            </div>
            <IslandButton
              label="Open Cilica"
              onClick={() => void api.showMainWindow().catch(() => undefined)}
            >
              <AppWindow className="nt-ico" strokeWidth={1.75} aria-hidden />
            </IslandButton>
            {autoStop !== null && (
              <IslandButton
                label="Keep recording"
                onClick={() => void api.keepRecording().catch(() => undefined)}
              >
                <Play className="nt-ico" strokeWidth={1.75} aria-hidden />
              </IslandButton>
            )}
            {live && <IslandStop onClick={() => void api.stopRecording().catch(() => undefined)} />}
          </>
        }
      />
    </div>
  );
}

export function IslandWindow() {
  useEffect(() => {
    document.documentElement.dataset.window = "island";
    // The browser preview has no recording of its own: start one so the island has something to show.
    if (!isTauri) {
      void api
        .getRecordingState()
        .then((s) =>
          s.phase === "idle"
            ? api.startRecording({ note_id: null, meeting_app: "Zoom", trigger: "manual" })
            : s,
        )
        .catch(() => undefined);
    }
  }, []);
  return <IslandApp />;
}
