import type { KeyboardEvent } from "react";

export interface SynonymPanelProps {
  /** The selected words, for the accessible name and the empty state. */
  word: string;
  /** Every synonym, best first; null while they load. */
  words: string[] | null;
  onPick: (word: string) => void;
  /** Esc. */
  onClose: () => void;
}

/**
 * Synonyms for the selected word, under the format bar's buttons (pass it as `FormatBar`'s
 * `panel`): a "Replace with" label over every synonym as a pill. It leaves focus where it is;
 * once in it, arrow keys move through the pills, Enter picks one, and ↑ from the first goes back
 * to the bar's buttons.
 */
export function SynonymPanel({ word, words, onPick, onClose }: SynonymPanelProps) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
      return;
    }
    const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[event.key];
    const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>("button"));
    const index = buttons.indexOf(event.target as HTMLButtonElement);
    if (index < 0) return;
    // ↑ from the first pill is the bar's: back to its buttons.
    if (index === 0 && event.key === "ArrowUp") return;
    let next: number | null = null;
    if (step) next = Math.min(Math.max(index + step, 0), buttons.length - 1);
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = buttons.length - 1;
    if (next === null) return;
    event.preventDefault();
    event.stopPropagation(); // the bar's own ←/→ move between its buttons
    buttons[next]?.focus();
  };

  return (
    // biome-ignore lint/a11y/useSemanticElements: a group of buttons, not form fields; a fieldset brings form styling.
    <div
      role="group"
      className="nt-synonyms"
      aria-label={`Synonyms for ${word}`}
      onKeyDown={onKeyDown}
    >
      {words === null ? (
        <p className="nt-synonyms-note">Looking up synonyms…</p>
      ) : words.length === 0 ? (
        <p className="nt-synonyms-note">No synonyms for “{word}”</p>
      ) : (
        <>
          <p className="nt-synonyms-heading">Replace with</p>
          <div className="nt-synonyms-words">
            {words.map((w) => (
              <button key={w} type="button" className="nt-synonyms-word" onClick={() => onPick(w)}>
                {w}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
