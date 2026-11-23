/**
 * Click a speaker to say who they are. The popup asks "Who is speaking?" and lists Saved people to
 * pick from (typing narrows the list); who they are now is the washed, checked row. Picking someone makes everything this speaker said in the
 * meeting theirs and teaches Cilica their voice; picking someone else later corrects it and takes
 * the wrong sample back. If someone else in the meeting already is that person, or is picked from
 * "In this meeting", the two become one speaker. A name nobody has yet can be added as a new
 * person, or used for this meeting only. Unlink (also for "You") makes them "Person N" again.
 * Arrow keys move through the list and Enter picks the highlighted row.
 */
import { Link2Off, UserPlus } from "lucide-react";
import { useState } from "react";
import type { Person, Speaker } from "@/ipc/types";
import { ChipButton, Icon, Popover, PopoverAnchor, PopoverContent, SpeakerButton } from "@/ui";
import { useSpeakerActions } from "./hooks";
import { type SpeakerLook, speakerLook } from "./model";
import { type WhoOption, WhoPicker } from "./WhoPicker";

export function SpeakerMenu({
  noteId,
  speaker,
  look,
  people,
  speakers,
  suggestedId,
  showChip = true,
}: {
  noteId: string;
  speaker: Speaker;
  look: SpeakerLook;
  people: readonly Person[];
  /** Everyone heard in this meeting, so this speaker can be merged into one of them. */
  speakers: readonly Speaker[];
  /** The Saved person Cilica thinks this speaker is, if it has a guess. */
  suggestedId?: string;
  /** "Name this speaker" shows once, on the speaker's first turn. */
  showChip?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const { rename, link, save, unlink, merge } = useSpeakerActions(noteId);

  // "You" before the meeting is tied to the You person still counts as them.
  const linked =
    people.find((p) => p.id === speaker.person_id) ??
    (speaker.is_self ? people.find((p) => p.is_self) : undefined);
  const suggested = linked ? undefined : people.find((p) => p.id === suggestedId);
  // Others heard in this meeting who are not a Saved person ("Person 2", a meeting-only name).
  // Saved people already here are picked from the list above, which merges them too.
  const others = speakers
    .filter((s) => s.id !== speaker.id && !s.person_id && !s.is_self)
    .map((s) => ({ speaker: s, look: speakerLook(s, people) }));

  const show = () => setOpen(true);
  const choose = (option: WhoOption) => {
    if (option.kind === "person") {
      link.mutate({ speakerId: speaker.id, personId: option.person.id });
    } else if (option.kind === "speaker") {
      merge.mutate({ from: speaker.id, into: option.speaker.id, name: option.look.name });
    } else {
      save.mutate({ speakerId: speaker.id, name: option.name });
    }
    setOpen(false);
  };

  const status = linked
    ? "Everything this speaker said in the meeting goes to who you pick."
    : suggested
      ? `Cilica thinks this is ${suggested.name}. Pick them to confirm, or pick someone else.`
      : "Pick who this is and everything they said in this meeting becomes theirs.";
  // Who they are now, when no row in the list says it: "Person 2" or a meeting-only name.
  const current = linked ? undefined : look.name;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverAnchor asChild>
        <SpeakerButton
          aria-haspopup="dialog"
          aria-expanded={open}
          data-state={open ? "open" : "closed"}
          onClick={show}
        >
          {look.name}
        </SpeakerButton>
      </PopoverAnchor>
      {look.unnamed && showChip && (
        <ChipButton icon={UserPlus} className="nt-name-chip" onClick={show}>
          Name this speaker
        </ChipButton>
      )}
      <PopoverContent label={`Who is ${look.name}?`}>
        <div className="nt-speaker-title">
          <span className="nt-speaker-question">Who is speaking?</span>
          {current && <span className="nt-speaker-current">{current}</span>}
        </div>
        <WhoPicker
          people={people}
          others={others}
          currentId={linked?.id}
          suggestedId={suggested?.id}
          fallbackName={!linked && !look.unnamed && !speaker.is_self ? look.name : ""}
          allowLabel={!linked}
          onChoose={choose}
          onLabel={(typed) => {
            if (typed !== speaker.label) rename.mutate({ speakerId: speaker.id, label: typed });
            setOpen(false);
          }}
          footer={
            linked && (
              <button
                type="button"
                className="nt-menu-item nt-anim"
                onClick={() => {
                  unlink.mutate(speaker.id);
                  setOpen(false);
                }}
              >
                <Icon icon={Link2Off} size="sm" motion="unlink" />
                <span className="nt-menu-label">Unlink</span>
              </button>
            )
          }
        />
        <p className="nt-popover-foot">{status}</p>
      </PopoverContent>
    </Popover>
  );
}
