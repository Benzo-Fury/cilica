/**
 * Click a speaker to say who they are. The popup shows who they are now, then Saved people to
 * pick from (typing narrows the list). Picking someone makes everything this speaker said in the
 * meeting theirs and teaches Cilica their voice; picking someone else later corrects it and takes
 * the wrong sample back. If someone else in the meeting already is that person, or is picked from
 * "In this meeting", the two become one speaker. A name nobody has yet can be added as a new
 * person, or used for this meeting only. Unlink (also for "You") makes them "Person N" again.
 * Arrow keys move through the list and Enter picks the highlighted row.
 */
import { Check, Link2Off, Pencil, UserPlus } from "lucide-react";
import { Fragment, type KeyboardEvent, useState } from "react";
import type { Person, Speaker } from "@/ipc/types";
import {
  Avatar,
  ChipButton,
  Field,
  Icon,
  Popover,
  PopoverAnchor,
  PopoverContent,
  SpeakerButton,
} from "@/ui";
import { useSpeakerActions } from "./hooks";
import { matchPeople, type SpeakerLook, speakerLook } from "./model";

type Option =
  | { kind: "person"; person: Person }
  | { kind: "speaker"; speaker: Speaker; look: SpeakerLook }
  | { kind: "add"; name: string };

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
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [navigated, setNavigated] = useState(false);
  const { rename, link, save, unlink, merge } = useSpeakerActions(noteId);

  // "You" before the meeting is tied to the You person still counts as them.
  const linked =
    people.find((p) => p.id === speaker.person_id) ??
    (speaker.is_self ? people.find((p) => p.is_self) : undefined);
  const suggested = linked ? undefined : people.find((p) => p.id === suggestedId);
  const typed = query.trim();
  const lower = typed.toLowerCase();
  const { people: matches, exact } = matchPeople(people, typed, linked?.id ?? suggested?.id);
  // Others heard in this meeting who are not a Saved person ("Person 2", a meeting-only name).
  // Saved people already here are picked from the list above, which merges them too.
  const others = speakers
    .filter((s) => s.id !== speaker.id && !s.person_id && !s.is_self)
    .map((s) => ({ kind: "speaker" as const, speaker: s, look: speakerLook(s, people) }))
    .filter((o) => !lower || o.look.name.toLowerCase().includes(lower));

  // A name typed by hand, or a meeting-only name given earlier, can become a Saved person.
  const newName = typed || (!linked && !look.unnamed && !speaker.is_self ? look.name : "");
  const taken = (name: string) =>
    people.some((p) => p.name.toLowerCase() === name.toLowerCase()) ||
    others.some((o) => o.look.name.toLowerCase() === name.toLowerCase());
  const options: Option[] = [
    ...matches.map((person) => ({ kind: "person" as const, person })),
    ...others,
    ...(newName && !exact && !taken(newName) ? [{ kind: "add" as const, name: newName }] : []),
  ];
  const firstOther = matches.length;
  // Nothing is highlighted until something is typed or an arrow key is pressed, so Enter
  // never picks by surprise.
  const highlighted = typed || navigated ? Math.min(active, options.length - 1) : -1;

  const show = () => {
    setQuery("");
    setActive(0);
    setNavigated(false);
    setOpen(true);
  };
  const choose = (option: Option) => {
    if (option.kind === "person") {
      link.mutate({ speakerId: speaker.id, personId: option.person.id });
    } else if (option.kind === "speaker") {
      merge.mutate({ from: speaker.id, into: option.speaker.id, name: option.look.name });
    } else {
      save.mutate({ speakerId: speaker.id, name: option.name });
    }
    setOpen(false);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (options.length === 0) return;
      const step = event.key === "ArrowDown" ? 1 : -1;
      const from = highlighted < 0 ? (step > 0 ? -1 : 0) : highlighted;
      setActive((from + step + options.length) % options.length);
      setNavigated(true);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const option = options[highlighted];
      if (option) choose(option);
    }
  };

  const status = linked
    ? "Everything they said in this meeting is theirs. Pick someone else to change it, or unlink them."
    : suggested
      ? `Cilica thinks this is ${suggested.name}. Pick them to confirm, or pick someone else.`
      : "Pick who this is and everything they said in this meeting becomes theirs.";

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
        <div className="nt-who">
          <Avatar name={look.name} color={look.color} size="sm" />
          <div className="nt-who-text">
            <div className="nt-who-name">{look.unnamed ? "Who is this?" : look.name}</div>
            <div className="nt-who-state">
              {linked ? "Saved person" : look.unnamed ? "Not named yet" : "Not a saved person"}
            </div>
          </div>
        </div>
        <p className="nt-popover-note">{status}</p>
        <Field
          size="sm"
          value={query}
          autoFocus
          placeholder={people.length > 0 ? "Find or add someone" : "Their name"}
          aria-label="Find or add someone"
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
        />
        {matches.length > 0 && <div className="nt-menu-heading">Saved people</div>}
        {options.length > 0 && (
          <div className="nt-popover-list">
            {options.map((option, index) =>
              option.kind === "person" ? (
                <button
                  key={option.person.id}
                  type="button"
                  className="nt-menu-item nt-anim"
                  data-highlighted={index === highlighted ? "" : undefined}
                  aria-current={linked?.id === option.person.id ? "true" : undefined}
                  onMouseMove={() => highlighted >= 0 && setActive(index)}
                  onClick={() => choose(option)}
                >
                  <Avatar name={option.person.name} color={option.person.color_index} size="xs" />
                  <span className="nt-menu-label">{option.person.name}</span>
                  {linked?.id === option.person.id ? (
                    <Icon icon={Check} size="sm" className="nt-accent-ico" />
                  ) : suggested?.id === option.person.id ? (
                    <span className="nt-menu-meta">Suggested</span>
                  ) : null}
                </button>
              ) : option.kind === "speaker" ? (
                <Fragment key={option.speaker.id}>
                  {index === firstOther && <div className="nt-menu-heading">In this meeting</div>}
                  <button
                    type="button"
                    className="nt-menu-item nt-anim"
                    data-highlighted={index === highlighted ? "" : undefined}
                    onMouseMove={() => highlighted >= 0 && setActive(index)}
                    onClick={() => choose(option)}
                  >
                    <Avatar name={option.look.name} color={option.look.color} size="xs" />
                    <span className="nt-menu-label">{option.look.name}</span>
                  </button>
                </Fragment>
              ) : (
                <button
                  key="add"
                  type="button"
                  className="nt-menu-item nt-anim"
                  data-highlighted={index === highlighted ? "" : undefined}
                  onMouseMove={() => highlighted >= 0 && setActive(index)}
                  onClick={() => choose(option)}
                >
                  <Icon icon={UserPlus} size="sm" motion="users" />
                  <span className="nt-menu-label">Add {option.name} as a new person</span>
                </button>
              ),
            )}
          </div>
        )}
        {(linked || (typed && !exact)) && <div className="nt-menu-sep" />}
        {typed && !exact && !linked && (
          <button
            type="button"
            className="nt-menu-item nt-anim"
            onClick={() => {
              if (typed !== speaker.label) rename.mutate({ speakerId: speaker.id, label: typed });
              setOpen(false);
            }}
          >
            <Icon icon={Pencil} size="sm" motion="pencil" />
            <span className="nt-menu-label">Call them {typed} in this meeting only</span>
          </button>
        )}
        {linked && (
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
        )}
      </PopoverContent>
    </Popover>
  );
}
