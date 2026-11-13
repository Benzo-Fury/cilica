import { type KeyboardEvent, type Ref, useLayoutEffect, useRef } from "react";
import { cn } from "./cn";

export interface TitleInputProps {
  value: string;
  onChange: (value: string) => void;
  onCommit?: (value: string) => void;
  /** Enter or ↓ at the end moves into the body. */
  onExit?: () => void;
  placeholder?: string;
  className?: string;
  ariaLabel?: string;
  readOnly?: boolean;
  ref?: Ref<HTMLTextAreaElement>;
}

/** The note title: display type, wraps and balances, grows with its content. */
export function TitleInput({
  value,
  onChange,
  onCommit,
  onExit,
  placeholder = "Untitled",
  className,
  ariaLabel = "Title",
  readOnly,
  ref,
}: TitleInputProps) {
  const inner = useRef<HTMLTextAreaElement | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-measure when the text changes
  useLayoutEffect(() => {
    const el = inner.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = `${el.scrollHeight}px`;
  }, [value]);

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    const el = event.currentTarget;
    if (
      event.key === "Enter" ||
      (event.key === "ArrowDown" && el.selectionStart === el.value.length)
    ) {
      event.preventDefault();
      onCommit?.(el.value);
      onExit?.();
    }
  };

  return (
    <textarea
      ref={(el) => {
        inner.current = el;
        if (typeof ref === "function") ref(el);
        else if (ref) ref.current = el;
      }}
      rows={1}
      value={value}
      readOnly={readOnly}
      aria-label={ariaLabel}
      placeholder={placeholder}
      className={cn("nt-note-title nt-title-input", className)}
      onChange={(e) => onChange(e.target.value.replace(/\n/g, " "))}
      onBlur={(e) => onCommit?.(e.target.value)}
      onKeyDown={onKeyDown}
    />
  );
}
