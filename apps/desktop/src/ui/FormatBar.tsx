/**
 * A small floating toolbar on strong glass, for formatting a text selection. The caller places
 * it (fixed `left`/`top`) and fills it with `IconButton`s, `FormatBarDivider`s, or a
 * `FormatBarField`. It never takes focus on mouse down, so the editor keeps its selection.
 */
import type { LucideIcon } from "lucide-react";
import {
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
  type Ref,
  useEffect,
  useRef,
} from "react";
import { Icon } from "./Icon";

export function FormatBar({
  label,
  side = "above",
  closing = false,
  style,
  onFocusLeave,
  children,
  ref,
}: {
  /** Accessible name, e.g. "Format text". */
  label: string;
  /** Which side of the selection it sits on; it grows from that edge. */
  side?: "above" | "below";
  /** Plays the exit fade; the caller unmounts it afterwards. */
  closing?: boolean;
  style?: CSSProperties;
  /** Focus moved from inside the bar to somewhere outside it. */
  onFocusLeave?: () => void;
  children: ReactNode;
  ref?: Ref<HTMLDivElement>;
}) {
  // ←/→ move between buttons, like any toolbar.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    const target = event.target as HTMLElement;
    if (target.tagName === "INPUT") return;
    const buttons = Array.from(
      event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"),
    );
    const index = buttons.indexOf(target as HTMLButtonElement);
    if (index < 0) return;
    event.preventDefault();
    const step = event.key === "ArrowRight" ? 1 : -1;
    buttons[(index + step + buttons.length) % buttons.length]?.focus();
  };

  return (
    <div
      ref={ref}
      role="toolbar"
      aria-label={label}
      aria-orientation="horizontal"
      data-side={side}
      data-closing={closing ? "" : undefined}
      className="nt-formatbar nt-glass-strong"
      style={style}
      onMouseDown={(e) => {
        if ((e.target as HTMLElement).tagName !== "INPUT") e.preventDefault();
      }}
      onKeyDown={onKeyDown}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) onFocusLeave?.();
      }}
    >
      {children}
    </div>
  );
}

export function FormatBarDivider() {
  return <span className="nt-formatbar-divider" aria-hidden="true" />;
}

/** An inline text field that replaces the buttons (a link's address). Enter submits, Esc cancels. */
export function FormatBarField({
  icon,
  label,
  placeholder,
  defaultValue,
  onSubmit,
  onCancel,
  children,
}: {
  icon: LucideIcon;
  label: string;
  placeholder?: string;
  defaultValue?: string;
  onSubmit: (value: string) => void;
  onCancel: () => void;
  /** Trailing buttons (apply, remove). */
  children?: ReactNode;
}) {
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);
  return (
    <form
      className="nt-formatbar-field"
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit(input.current?.value ?? "");
      }}
    >
      <Icon icon={icon} />
      <input
        ref={input}
        aria-label={label}
        placeholder={placeholder}
        defaultValue={defaultValue}
        spellCheck={false}
        autoComplete="off"
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            onCancel();
          }
        }}
      />
      {children}
    </form>
  );
}
