import { Link2 } from "lucide-react";
import type { CSSProperties } from "react";
import { Icon } from "./Icon";
import { Kbd } from "./Kbd";

export interface LinkHintProps {
  /** The note it would link to, as shown. */
  title: string;
  /** Where it sits (fixed, under the typed words). */
  style?: CSSProperties;
  onAccept: () => void;
}

/**
 * The quiet offer under words you just typed that name a note: "Link to Cytoplasm  Tab". It never
 * takes focus (mouse down keeps the caret in the editor), so typing straight on dismisses it.
 */
export function LinkHint({ title, style, onAccept }: LinkHintProps) {
  return (
    <button
      type="button"
      tabIndex={-1}
      className="nt-link-hint nt-glass-strong"
      style={style}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onAccept}
      aria-label={`Link to ${title}`}
    >
      <Icon icon={Link2} size="sm" />
      <span className="nt-link-hint-label">
        Link to <span className="nt-link-hint-title">{title}</span>
      </span>
      <Kbd keys={["Tab"]} />
    </button>
  );
}
