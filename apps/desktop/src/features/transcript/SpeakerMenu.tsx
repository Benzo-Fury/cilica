/**
 * Click a speaker to name them: rename for this meeting, link to a Saved person (their voice
 * becomes one of that person's samples, and picking someone else takes it back), or save them as
 * a new person. Picking the person Margin already guessed confirms it and teaches it too. A
 * linked person's colour wins.
 */
import { Check, Link2Off, UserPlus } from "lucide-react";
import { type KeyboardEvent, useState } from "react";
import type { Person, Speaker } from "@/ipc/types";
import {
  Avatar,
  Button,
  ChipButton,
  Field,
  Icon,
  Popover,
  PopoverAnchor,
  PopoverContent,
  SpeakerButton,
} from "@/ui";
import { useSpeakerActions } from "./hooks";
import type { SpeakerLook } from "./model";

export function SpeakerMenu({
  noteId,
  speaker,
  look,
  people,
  showChip = true,
}: {
  noteId: string;
  speaker: Speaker;
  look: SpeakerLook;
  people: readonly Person[];
  /** "Name this speaker" shows once, on the speaker's first turn. */
  showChip?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState(look.unnamed ? "" : look.name);
  const { rename, link, save } = useSpeakerActions(noteId);
  const linked = people.find((p) => p.id === speaker.person_id);
  const trimmed = name.trim();

  const show = () => {
    setName(look.unnamed ? "" : look.name);
    setOpen(true);
  };
  const commitRename = () => {
    if (!trimmed || trimmed === speaker.label) return setOpen(false);
    rename.mutate({ speakerId: speaker.id, label: trimmed });
    setOpen(false);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      commitRename();
    }
  };

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
      <PopoverContent label={`Name ${look.name}`}>
        <div className="nt-popover-title">Who is {look.name}?</div>
        <p className="nt-popover-note">Picking a saved person teaches Margin their voice.</p>
        <Field
          size="sm"
          value={name}
          autoFocus
          placeholder="Their name"
          aria-label="Speaker name"
          onChange={(e) => setName(e.target.value)}
          onKeyDown={onKeyDown}
        />
        {people.length > 0 && (
          <>
            <div className="nt-menu-heading">Saved people</div>
            <div className="nt-popover-list">
              {people.map((person) => (
                <button
                  key={person.id}
                  type="button"
                  className="nt-menu-item"
                  onClick={() => {
                    link.mutate({ speakerId: speaker.id, personId: person.id });
                    setOpen(false);
                  }}
                >
                  <Avatar name={person.name} color={person.color_index} size="xs" />
                  <span className="nt-menu-label">{person.name}</span>
                  {linked?.id === person.id && (
                    <Icon icon={Check} size="sm" className="nt-accent-ico" />
                  )}
                </button>
              ))}
            </div>
          </>
        )}
        <div className="nt-menu-sep" />
        {linked ? (
          <button
            type="button"
            className="nt-menu-item"
            onClick={() => {
              link.mutate({ speakerId: speaker.id, personId: null });
              setOpen(false);
            }}
          >
            <Icon icon={Link2Off} size="sm" />
            <span className="nt-menu-label">Unlink from {linked.name}</span>
          </button>
        ) : (
          <button
            type="button"
            className="nt-menu-item"
            disabled={!trimmed}
            onClick={() => {
              save.mutate({ speakerId: speaker.id, name: trimmed });
              setOpen(false);
            }}
          >
            <Icon icon={UserPlus} size="sm" />
            <span className="nt-menu-label">
              {trimmed ? `Save as ${trimmed}` : "Save as person"}
            </span>
          </button>
        )}
        <div className="nt-popover-actions">
          <Button size="sm" variant="ghost" onClick={() => setOpen(false)}>
            Cancel
          </Button>
          <Button size="sm" variant="primary" disabled={!trimmed} onClick={commitRename}>
            Rename
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
