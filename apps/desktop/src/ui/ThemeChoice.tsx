/**
 * Theme picker cards (docs/design-system/components/ThemeChoice): a radio group of miniature
 * windows, each painted with its own theme's tokens and scene, so the card is the preview.
 */
import type { ReactNode } from "react";
import { SceneArt, type SceneKind } from "./SceneArt";

export function ThemeChoices({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="nt-theme-choices" role="radiogroup" aria-label={label}>
      {children}
    </div>
  );
}

export interface ThemeChoiceProps {
  title: string;
  /** The bases to show. Two (light, dark) split the card down the middle, for System. */
  schemes: readonly ("light" | "dark")[];
  /** The `data-look` whose tokens the miniature takes on top of its base. */
  look?: string;
  scene?: SceneKind | null;
  checked: boolean;
  onSelect: () => void;
}

export function ThemeChoice({ title, schemes, look, scene, checked, onSelect }: ThemeChoiceProps) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a styled button keeps the design-system look and focus ring
    <button
      type="button"
      role="radio"
      aria-checked={checked}
      className="nt-theme-choice"
      onClick={onSelect}
    >
      <span className="nt-theme-preview" aria-hidden>
        {schemes.map((scheme) => (
          <span
            key={scheme}
            className="nt-theme-pane"
            data-theme={scheme}
            data-look={look}
            data-scene={scene ?? undefined}
          >
            {scene && <SceneArt scene={scene} still pixel={1} />}
            {scene && <span className="nt-theme-pane-veil" />}
            <span className="nt-theme-pane-side">
              <i />
              <i data-selected />
              <i />
              <i />
            </span>
            <span className="nt-theme-pane-note">
              <b />
              <i />
              <i />
              <i />
              <u />
            </span>
          </span>
        ))}
      </span>
      <span className="nt-theme-choice-title">{title}</span>
    </button>
  );
}
