/**
 * Recording parts shared by features: the tabular timer and the recording control in the note's
 * title band (RecordingStatus doc).
 */
import { Mic, Pause, Play, Square } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
import type { ReactNode } from "react";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { Button } from "./Button";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { Shimmer } from "./Skeleton";
import { Tooltip } from "./Tooltip";

/** A tabular, monospaced timer. */
export function Timecode({ children, className }: { children: ReactNode; className?: string }) {
  return <span className={cn("nt-mono nt-timer", className)}>{children}</span>;
}

export type RecordingControlStatus =
  | { kind: "starting" }
  | { kind: "finishing"; label: string }
  | {
      kind: "live";
      /** Already formatted, "12:04". */
      elapsed: string;
      paused: boolean;
      /** Whole seconds until a detected meeting's recording stops on its own. */
      stopsIn: number | null;
    };

export interface RecordingControlProps {
  status: RecordingControlStatus;
  onPause: () => void;
  /** The feature decides whether to ask first (a meeting still in its call). */
  onStop: () => void;
  onKeep: () => void;
  stopShortcut?: readonly string[];
}

/**
 * The recording control: a status pill, then round Pause and Stop buttons. Neutral like the rest
 * of the band: no red in the app, and nothing pulses. Wrap in `AnimatePresence`.
 */
export function RecordingControl({
  status,
  onPause,
  onStop,
  onKeep,
  stopShortcut,
}: RecordingControlProps) {
  const reduced = useReducedMotion();
  const live = status.kind === "live" ? status : null;
  const paused = live?.paused ?? false;
  return (
    <motion.span
      className="nt-recctl"
      initial={reduced ? { opacity: 0 } : { opacity: 0, scale: 0.9 }}
      animate={{ opacity: 1, scale: 1, transition: reduced ? reducedTransition : springs.bouncy }}
      exit={{ opacity: 0, scale: reduced ? 1 : 0.94, transition: tweens.exit }}
    >
      <span
        className={cn(
          "nt-recctl-status",
          paused && "nt-recctl-status-paused",
          status.kind === "finishing" && "nt-recctl-status-plain",
        )}
        aria-live={live?.stopsIn != null || status.kind === "finishing" ? "polite" : undefined}
      >
        {status.kind !== "finishing" && (
          <span className="nt-recctl-swap" data-on={paused ? "b" : "a"} aria-hidden>
            <Icon icon={Mic} size="sm" />
            <Icon icon={Pause} size="sm" />
          </span>
        )}
        {status.kind === "starting" && <Shimmer>Starting…</Shimmer>}
        {status.kind === "finishing" && <Shimmer>{status.label}</Shimmer>}
        {live &&
          (live.stopsIn !== null ? (
            <span>Stops in {live.stopsIn}s</span>
          ) : (
            <span>
              <span className="nt-sr-only">{paused ? "Paused at" : "Recording for"} </span>
              {paused && <span aria-hidden>Paused · </span>}
              <Timecode>{live.elapsed}</Timecode>
            </span>
          ))}
      </span>
      {live && (
        <>
          {live.stopsIn !== null && (
            <Button size="sm" variant="secondary" onClick={onKeep}>
              Keep recording
            </Button>
          )}
          <Tooltip label={paused ? "Resume recording" : "Pause recording"}>
            <button
              type="button"
              className="nt-recctl-btn"
              aria-label={paused ? "Resume recording" : "Pause recording"}
              onClick={onPause}
            >
              <span className="nt-recctl-swap" data-on={paused ? "b" : "a"} aria-hidden>
                <Icon icon={Pause} size="sm" className="nt-recctl-solid" />
                <Icon icon={Play} size="sm" className="nt-recctl-solid" />
              </span>
            </button>
          </Tooltip>
          <Tooltip label="Stop recording" shortcut={stopShortcut}>
            <button
              type="button"
              className="nt-recctl-btn nt-recctl-stop"
              aria-label="Stop recording"
              onClick={onStop}
            >
              <Icon icon={Square} size="sm" className="nt-recctl-solid" />
            </button>
          </Tooltip>
        </>
      )}
    </motion.span>
  );
}
