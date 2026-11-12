import { useEffect, useRef, useState } from "react";

/**
 * Renaming in place inside a sidebar row (new category, rename): the name stays where it is, in
 * the same type, with an amber underline that hugs the text. Enter commits, Esc or an empty
 * blur cancels.
 */
export function InlineName({
  initial,
  placeholder,
  label,
  onCommit,
  onCancel,
}: {
  initial: string;
  placeholder: string;
  label: string;
  onCommit: (name: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(initial);
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  const finish = (commit: boolean) => {
    if (done.current) return;
    done.current = true;
    const name = value.trim();
    if (commit && name && name !== initial) onCommit(name);
    else onCancel();
  };
  return (
    <span className="nt-inline-name" data-value={value || placeholder}>
      <input
        ref={ref}
        value={value}
        aria-label={label}
        placeholder={placeholder}
        spellCheck={false}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter") finish(true);
          if (e.key === "Escape") {
            e.preventDefault();
            finish(false);
          }
        }}
        onBlur={() => finish(true)}
        onClick={(e) => e.stopPropagation()}
        onPointerDown={(e) => e.stopPropagation()}
      />
    </span>
  );
}
