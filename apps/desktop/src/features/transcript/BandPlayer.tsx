/** The recording's player in the note's title band: play / pause with the time. */
import { Pause, Play } from "lucide-react";
import { clock } from "@/lib/format";
import { Icon, Tooltip } from "@/ui";

export interface BandPlayerProps {
  playing: boolean;
  positionMs: number;
  durationMs: number;
  onToggle: () => void;
}

export function BandPlayer({ playing, positionMs, durationMs, onToggle }: BandPlayerProps) {
  const label = playing ? "Pause the recording" : "Play the recording";
  return (
    <div className="nt-band-player">
      <Tooltip label={label} side="bottom">
        <button
          type="button"
          className="nt-btn nt-btn-sm nt-btn-secondary nt-band-play"
          aria-label={label}
          onClick={onToggle}
        >
          <Icon icon={playing ? Pause : Play} size="sm" />
          <span className="nt-mono nt-band-player-time">
            {clock(positionMs)} / {clock(durationMs)}
          </span>
        </button>
      </Tooltip>
    </div>
  );
}
