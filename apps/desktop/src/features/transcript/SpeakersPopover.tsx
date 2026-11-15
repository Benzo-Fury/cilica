/**
 * The "N speakers" chip above the transcript. Click it for everyone heard in the meeting, with
 * how much of the talking was theirs; pick someone to jump to where they first speak.
 */
import { useRef, useState } from "react";
import { plural } from "@/lib/format";
import { Avatar, AvatarStack, ChipButton, Popover, PopoverContent, PopoverTrigger } from "@/ui";
import type { SpeakerLook } from "./model";

export interface SpeakerEntry {
  key: string;
  look: SpeakerLook;
  /** Their part of the talking, 0 to 1. */
  share: number;
  firstTurn: number;
}

export function SpeakersPopover({
  speakers,
  live,
  onPick,
}: {
  speakers: readonly SpeakerEntry[];
  /** While recording the shares are still moving, so they are left out. */
  live: boolean;
  onPick: (turnIndex: number) => void;
}) {
  const [open, setOpen] = useState(false);
  // The jump waits for the popover to close: handing focus back to the chip would scroll it away.
  const picked = useRef<number | null>(null);
  const stack = speakers.slice(0, 3).map((s) => ({ name: s.look.name, color: s.look.color }));
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <ChipButton leading={<AvatarStack people={stack} />} aria-haspopup="dialog">
          {plural(speakers.length, "speaker")}
        </ChipButton>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        label="Speakers"
        className="nt-speakers-pop"
        onCloseAutoFocus={(event) => {
          if (picked.current === null) return;
          event.preventDefault();
          onPick(picked.current);
          picked.current = null;
        }}
      >
        <div className="nt-menu-heading">In this meeting</div>
        <div className="nt-popover-list">
          {speakers.map((s) => (
            <button
              key={s.key}
              type="button"
              className="nt-menu-item nt-anim"
              title="Show where they first speak"
              onClick={() => {
                picked.current = s.firstTurn;
                setOpen(false);
              }}
            >
              <Avatar name={s.look.name} color={s.look.color} size="xs" />
              <span className="nt-menu-label">{s.look.name}</span>
              {!live && <span className="nt-menu-meta">{Math.round(s.share * 100)}%</span>}
            </button>
          ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}
