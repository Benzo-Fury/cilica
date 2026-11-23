/**
 * "Who said this?" for words selected in the transcript: the same list as the speaker menu
 * (Saved people, others in this meeting, a new person, a name for this meeting only). Picking
 * someone moves just those words to them, splitting the lines they were in; if they are a Saved
 * person, Cilica learns their voice from the words. ⌘C still copies the words while it is open.
 */
import { useEffect, useMemo } from "react";
import type { Person, Segment, Speaker, WordsTarget } from "@/ipc/types";
import { Popover, PopoverAnchor, PopoverContent } from "@/ui";
import { useMoveWords } from "./hooks";
import { selectedSpeakers, speakerLook } from "./model";
import type { WordSelection } from "./useWordSelection";
import { type WhoOption, WhoPicker } from "./WhoPicker";

export function WordsMenu({
  noteId,
  selection,
  segments,
  speakers,
  people,
  onClose,
}: {
  noteId: string;
  selection: WordSelection | null;
  segments: readonly Segment[];
  speakers: readonly Speaker[];
  people: readonly Person[];
  /** `moved`: the words went to someone, so the page selection goes too. */
  onClose: (moved: boolean) => void;
}) {
  const move = useMoveWords(noteId);
  const anchor = useMemo(
    () => ({
      current: selection
        ? { getBoundingClientRect: () => selection.range.getBoundingClientRect() }
        : null,
    }),
    [selection],
  );

  // Everyone the words are from now: when that is one speaker, they are "who it is now".
  const owners = selection ? selectedSpeakers(segments, selection.from, selection.to) : [];
  const owner =
    owners.length === 1 && owners[0] ? speakers.find((s) => s.id === owners[0]) : undefined;
  const ownerPerson =
    owner &&
    (people.find((p) => p.id === owner.person_id) ??
      (owner.is_self ? people.find((p) => p.is_self) : undefined));
  const others = speakers
    .filter((s) => s.id !== owner?.id && !s.person_id && !s.is_self)
    .map((s) => ({ speaker: s, look: speakerLook(s, people) }));

  // ⌘C in the empty field copies the selected words, as it would without the menu.
  useEffect(() => {
    if (!selection) return;
    const onCopy = (event: ClipboardEvent) => {
      const field = document.activeElement;
      if (!(field instanceof HTMLInputElement) || field.selectionStart !== field.selectionEnd)
        return;
      event.clipboardData?.setData("text/plain", selection.text);
      event.preventDefault();
    };
    document.addEventListener("copy", onCopy);
    return () => document.removeEventListener("copy", onCopy);
  }, [selection]);

  const send = (target: WordsTarget, name: string) => {
    if (!selection) return;
    move.mutate({
      from: { segment_id: selection.from.segmentId, word: selection.from.word.index },
      to: { segment_id: selection.to.segmentId, word: selection.to.word.index },
      target,
      name,
    });
    onClose(true);
  };
  const choose = (option: WhoOption) => {
    if (option.kind === "person") {
      send({ type: "person", person_id: option.person.id }, option.person.name);
    } else if (option.kind === "speaker") {
      send({ type: "speaker", speaker_id: option.speaker.id }, option.look.name);
    } else {
      send({ type: "new_person", name: option.name }, option.name);
    }
  };

  return (
    <Popover open={Boolean(selection)} onOpenChange={(open) => !open && onClose(false)}>
      <PopoverAnchor virtualRef={anchor} />
      <PopoverContent label="Who said this?" onCloseAutoFocus={(event) => event.preventDefault()}>
        <div className="nt-speaker-title">
          <span className="nt-speaker-question">Who said this?</span>
          {selection && <span className="nt-speaker-current">“{selection.text}”</span>}
        </div>
        <WhoPicker
          key={selection ? `${selection.from.segmentId}:${selection.from.word.index}` : "none"}
          people={people}
          others={others}
          currentId={ownerPerson?.id}
          allowLabel
          onChoose={choose}
          onLabel={(name) => send({ type: "label", name }, name)}
        />
        <p className="nt-popover-foot">
          Pick who said these words and they become theirs. If they're a Saved person, Cilica learns
          their voice from them too.
        </p>
      </PopoverContent>
    </Popover>
  );
}
