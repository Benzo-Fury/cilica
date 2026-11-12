/**
 * The recording's player in the note's title band: play / pause with the time, and an info
 * button whose popover holds the scrubber, skip and speed controls and what is known about the
 * recording (when, how long, who spoke how much, where the audio came from).
 */
import { Info, Pause, Play } from "lucide-react";
import { clock } from "@/lib/format";
import {
  Icon,
  IconButton,
  PlaybackBar,
  Popover,
  PopoverContent,
  PopoverTrigger,
  Tooltip,
} from "@/ui";

export interface RecordingDetails {
  recordedAt: string | null;
  durationMs: number;
  speakers: { name: string; color: number; share: number }[];
  source: string;
}

export interface BandPlayerProps {
  playing: boolean;
  positionMs: number;
  durationMs: number;
  rate: number;
  onToggle: () => void;
  onSeek: (ms: number) => void;
  onSkip: (deltaMs: number) => void;
  onRate: () => void;
  details: RecordingDetails;
}

function recorded(at: string | null): string | null {
  if (!at) return null;
  const date = new Date(at);
  if (Number.isNaN(date.getTime())) return null;
  return date.toLocaleString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function BandPlayer({ details, ...player }: BandPlayerProps) {
  const when = recorded(details.recordedAt);
  return (
    <div className="nt-band-player">
      <Tooltip label={player.playing ? "Pause the recording" : "Play the recording"} side="bottom">
        <button
          type="button"
          className="nt-btn nt-btn-sm nt-btn-secondary nt-band-play"
          aria-label={player.playing ? "Pause the recording" : "Play the recording"}
          onClick={player.onToggle}
        >
          <Icon icon={player.playing ? Pause : Play} size="sm" />
          <span className="nt-mono nt-band-player-time">
            {clock(player.positionMs)} / {clock(player.durationMs)}
          </span>
        </button>
      </Tooltip>
      <Popover>
        <PopoverTrigger asChild>
          <IconButton icon={Info} label="Recording details" />
        </PopoverTrigger>
        <PopoverContent align="center" label="Recording" className="nt-recinfo">
          <PlaybackBar
            playing={player.playing}
            positionMs={player.positionMs}
            durationMs={player.durationMs}
            rate={player.rate}
            format={clock}
            onToggle={player.onToggle}
            onSeek={player.onSeek}
            onSkip={player.onSkip}
            onRate={player.onRate}
          />
          <dl className="nt-recinfo-list">
            {when && (
              <>
                <dt>Recorded</dt>
                <dd>{when}</dd>
              </>
            )}
            <dt>Length</dt>
            <dd>{clock(details.durationMs)}</dd>
            {details.speakers.length > 0 && (
              <>
                <dt>Speakers</dt>
                <dd className="nt-recinfo-speakers">
                  {details.speakers.map((s) => (
                    <span key={s.name} data-speaker={s.color}>
                      <b>{s.name}</b> {Math.round(s.share * 100)}%
                    </span>
                  ))}
                </dd>
              </>
            )}
            <dt>Audio from</dt>
            <dd>{details.source}</dd>
          </dl>
        </PopoverContent>
      </Popover>
    </div>
  );
}
