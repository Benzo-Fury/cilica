/**
 * The "who is this" list inside the speaker popovers: a field that finds or adds someone, Saved
 * people (typing narrows them), others heard in this meeting, adding a new person, and a name for
 * this meeting only. Arrow keys move through the list and Enter picks the highlighted row;
 * nothing is highlighted until something is typed or an arrow key is pressed, so Enter never
 * picks by surprise. Used by the speaker name's menu and by the menu for selected words.
 */
import { Check, Pencil, UserPlus } from "lucide-react";
import { Fragment, type KeyboardEvent, type ReactNode, useState } from "react";
import type { Person, Speaker } from "@/ipc/types";
import { Avatar, Field, Icon } from "@/ui";
import { matchPeople, type SpeakerLook } from "./model";

export type WhoOption =
  | { kind: "person"; person: Person }
  | { kind: "speaker"; speaker: Speaker; look: SpeakerLook }
  | { kind: "add"; name: string };

export function WhoPicker({
  people,
  others,
  currentId,
  suggestedId,
  fallbackName = "",
  allowLabel,
  onChoose,
  onLabel,
  footer,
}: {
  people: readonly Person[];
  /** Others heard in this meeting who are not a Saved person, offered under "In this meeting". */
  others: readonly { speaker: Speaker; look: SpeakerLook }[];
  /** The Saved person it is now: listed first, with a check. */
  currentId?: string;
  /** The Saved person Cilica thinks it is: listed first, marked "Suggested". */
  suggestedId?: string;
  /** A name to offer as a new person when nothing is typed (a meeting-only name given earlier). */
  fallbackName?: string;
  /** Offer "Call them … in this meeting only" for a typed name nobody has. */
  allowLabel: boolean;
  onChoose: (option: WhoOption) => void;
  onLabel: (name: string) => void;
  /** Rows after the separator, such as Unlink. */
  footer?: ReactNode;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [navigated, setNavigated] = useState(false);

  const typed = query.trim();
  const lower = typed.toLowerCase();
  const { people: matches, exact } = matchPeople(people, typed, currentId ?? suggestedId);
  const shown = others
    .map((o) => ({ kind: "speaker" as const, ...o }))
    .filter((o) => !lower || o.look.name.toLowerCase().includes(lower));

  // A name typed by hand, or a meeting-only name given earlier, can become a Saved person.
  const newName = typed || fallbackName;
  const taken = (name: string) =>
    people.some((p) => p.name.toLowerCase() === name.toLowerCase()) ||
    shown.some((o) => o.look.name.toLowerCase() === name.toLowerCase());
  const options: WhoOption[] = [
    ...matches.map((person) => ({ kind: "person" as const, person })),
    ...shown,
    ...(newName && !exact && !taken(newName) ? [{ kind: "add" as const, name: newName }] : []),
  ];
  const firstOther = matches.length;
  const highlighted = typed || navigated ? Math.min(active, options.length - 1) : -1;
  const showLabel = Boolean(typed && !exact && allowLabel);

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
      if (option) onChoose(option);
    }
  };

  return (
    <>
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
                aria-current={currentId === option.person.id ? "true" : undefined}
                data-current={currentId === option.person.id ? "" : undefined}
                onMouseMove={() => highlighted >= 0 && setActive(index)}
                onClick={() => onChoose(option)}
              >
                <Avatar name={option.person.name} color={option.person.color_index} size="xs" />
                <span className="nt-menu-label">{option.person.name}</span>
                {currentId === option.person.id ? (
                  <Icon icon={Check} size="sm" className="nt-accent-ico" />
                ) : suggestedId === option.person.id ? (
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
                  onClick={() => onChoose(option)}
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
                onClick={() => onChoose(option)}
              >
                <Icon icon={UserPlus} size="sm" motion="users" />
                <span className="nt-menu-label">Add {option.name} as a new person</span>
              </button>
            ),
          )}
        </div>
      )}
      {(footer || showLabel) && <div className="nt-menu-sep" />}
      {showLabel && (
        <button type="button" className="nt-menu-item nt-anim" onClick={() => onLabel(typed)}>
          <Icon icon={Pencil} size="sm" motion="pencil" />
          <span className="nt-menu-label">Call them {typed} in this meeting only</span>
        </button>
      )}
      {footer}
    </>
  );
}
