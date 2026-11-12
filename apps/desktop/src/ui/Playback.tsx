/**
 * Playback controls for a recorded meeting (AppTranscript mockup): they take over the capsule's
 * place after recording. Back 15s, play/pause, forward 15s, time, a scrubber and speed.
 */
import { Pause, Play, RotateCcw, RotateCw } from "lucide-react";
import { type CSSProperties, type KeyboardEvent, type PointerEvent, useRef, useState } from "react";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { IconButton } from "./IconButton";
import { Tooltip } from "./Tooltip";

export interface PlaybackBarProps {
  playing: boolean;
  positionMs: number;
  durationMs: number;
  rate: number;
  /** Formats milliseconds as a timecode. */
  format: (ms: number) => string;
  onToggle: () => void;
  onSeek: (ms: number) => void;
  onSkip: (deltaMs: number) => void;
  onRate: () => void;
  playShortcut?: readonly string[];
}

export function PlaybackBar({
  playing,
  positionMs,
  durationMs,
  rate,
  format,
  onToggle,
  onSeek,
  onSkip,
  onRate,
  playShortcut,
}: PlaybackBarProps) {
  const track = useRef<HTMLDivElement>(null);
  const [scrubbing, setScrubbing] = useState<number | null>(null);
  const shown = scrubbing ?? positionMs;
  const fraction = durationMs > 0 ? Math.min(1, Math.max(0, shown / durationMs)) : 0;

  const at = (event: PointerEvent<HTMLDivElement>) => {
    const rect = track.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) return 0;
    return Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width)) * durationMs;
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? 30_000 : 5_000;
    if (event.key === "ArrowLeft") onSkip(-step);
    else if (event.key === "ArrowRight") onSkip(step);
    else if (event.key === "Home") onSeek(0);
    else if (event.key === "End") onSeek(durationMs);
    else return;
    event.preventDefault();
  };

  return (
    <div className="nt-playback">
      <IconButton
        icon={RotateCcw}
        label="Back 15 seconds"
        tooltipSide="top"
        onClick={() => onSkip(-15_000)}
      />
      <Tooltip label={playing ? "Pause" : "Play"} shortcut={playShortcut} side="top">
        <button
          type="button"
          className="nt-send nt-play"
          aria-label={playing ? "Pause" : "Play"}
          onClick={onToggle}
        >
          <Icon
            icon={playing ? Pause : Play}
            size="sm"
            className={cn(!playing && "nt-play-glyph")}
          />
        </button>
      </Tooltip>
      <IconButton
        icon={RotateCw}
        label="Forward 15 seconds"
        tooltipSide="top"
        onClick={() => onSkip(15_000)}
      />
      <span className="nt-mono nt-playback-time">
        {format(shown)} / {format(durationMs)}
      </span>
      {/* biome-ignore lint/a11y/useSemanticElements: a custom scrubber with full keyboard support */}
      <div
        ref={track}
        className="nt-scrub"
        role="slider"
        tabIndex={0}
        aria-label="Playback position"
        aria-valuemin={0}
        aria-valuemax={Math.round(durationMs / 1000)}
        aria-valuenow={Math.round(shown / 1000)}
        aria-valuetext={format(shown)}
        data-scrubbing={scrubbing !== null || undefined}
        style={{ "--p": fraction } as CSSProperties}
        onKeyDown={onKeyDown}
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          setScrubbing(at(e));
        }}
        onPointerMove={(e) => {
          if (scrubbing !== null) setScrubbing(at(e));
        }}
        onPointerUp={(e) => {
          if (scrubbing === null) return;
          onSeek(at(e));
          setScrubbing(null);
        }}
        onPointerCancel={() => setScrubbing(null)}
      >
        <span className="nt-scrub-fill" />
        <span className="nt-scrub-knob" />
      </div>
      <Tooltip label="Playback speed" side="top">
        <button
          type="button"
          className="nt-chip nt-speed"
          onClick={onRate}
          aria-label={`Playback speed ${rate}×`}
        >
          {rate}×
        </button>
      </Tooltip>
    </div>
  );
}
