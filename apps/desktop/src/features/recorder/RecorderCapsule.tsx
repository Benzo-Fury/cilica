/**
 * The recorder capsule, bottom-centre over the canvas: starting → live (breathing dot, timer,
 * level meter, Stop) → "Writing notes…". Starting a recording lives in the transcript panel
 * and the sidebar; the capsule never shows while idle.
 */
import { Mic, Volume2 } from "lucide-react";
import { AnimatePresence, LayoutGroup } from "motion/react";
import type { CSSProperties } from "react";
import { useShortcut } from "@/app/keybinds";
import { clock } from "@/lib/format";
import {
  Button,
  Capsule,
  CapsuleContent,
  CapsuleDock,
  CapsuleDot,
  Icon,
  LevelMeter,
  Shimmer,
  StopButton,
  Timecode,
  Tooltip,
} from "@/ui";
import {
  type CapsuleMode,
  capsuleMode,
  useAudioLevels,
  useClock,
  useJitter,
  useKeepRecording,
  useRecordingState,
  useStopRecording,
} from "./hooks";

/**
 * Where the capsule shows: on every page while recording or finishing. Recording starts from
 * the transcript panel.
 */
function useVisible(mode: CapsuleMode) {
  return mode.kind !== "idle";
}

export function RecorderCapsule() {
  const state = useRecordingState();
  const live = state.phase === "recording" || state.phase === "starting";
  const now = useClock(state.phase !== "idle", state.auto_stop_at ? 250 : 1000);
  const mode = capsuleMode(state, now);
  const levels = useAudioLevels(state.phase === "recording");
  const jitter = useJitter(levels);
  const stop = useStopRecording();
  const keep = useKeepRecording();
  const recordKeys = useShortcut("recording.toggle");
  const visible = useVisible(mode);
  if (!visible) return null;
  const level = Math.max(levels.mic ?? 0, levels.system ?? 0);

  return (
    <CapsuleDock>
      <LayoutGroup id="capsule">
        <Capsule label="Recorder" live={live}>
          <AnimatePresence mode="popLayout" initial={false}>
            {mode.kind === "starting" && (
              <CapsuleContent key="starting">
                <CapsuleDot />
                <span className="nt-capsule-label">
                  <Shimmer>Starting…</Shimmer>
                </span>
              </CapsuleContent>
            )}
            {mode.kind === "recording" && (
              <CapsuleContent key="recording">
                <CapsuleDot />
                <Timecode>
                  <span className="nt-sr-only">Recording for </span>
                  {clock(mode.elapsedMs)}
                </Timecode>
                <LevelMeter level={level} jitter={jitter} label="Input level" />
                {mode.autoStopInMs !== null ? (
                  <span className="nt-countdown" aria-live="polite">
                    <span
                      key={state.auto_stop_at}
                      className="nt-ring"
                      style={{ "--t": `${Math.ceil(mode.autoStopInMs / 1000)}s` } as CSSProperties}
                      aria-hidden
                    >
                      <svg viewBox="0 0 28 28" aria-hidden="true">
                        <circle cx="14" cy="14" r="12" />
                      </svg>
                    </span>
                    Stops in {Math.ceil(mode.autoStopInMs / 1000)}s
                    <Button size="sm" variant="secondary" onClick={() => void keep()}>
                      Keep recording
                    </Button>
                  </span>
                ) : (
                  <span className="nt-capsule-extra">
                    <span className="nt-chip nt-capsule-source">
                      <Icon icon={Mic} size="sm" />
                      Microphone
                    </span>
                    <span className="nt-chip nt-capsule-source">
                      <Icon icon={Volume2} size="sm" />
                      {state.meeting_app ?? "Call audio"}
                    </span>
                  </span>
                )}
                <Tooltip label="Stop recording" shortcut={recordKeys} side="top">
                  <StopButton aria-label="Stop recording" onClick={() => void stop()} />
                </Tooltip>
              </CapsuleContent>
            )}
            {mode.kind === "finishing" && (
              <CapsuleContent key="finishing">
                <StopButton done disabled aria-label="Recording stopped" />
                <span className="nt-capsule-label" aria-live="polite">
                  <Shimmer>{mode.label}</Shimmer>
                  <span className="nt-sr-only">
                    {mode.step === "stopping"
                      ? "Finishing the transcript"
                      : mode.step === "diarizing"
                        ? "Telling speakers apart"
                        : "Writing the summary"}
                  </span>
                </span>
              </CapsuleContent>
            )}
          </AnimatePresence>
        </Capsule>
      </LayoutGroup>
    </CapsuleDock>
  );
}
