/**
 * The search bar. One look, two forms: `SearchButton` opens somewhere else to search (the
 * sidebar's Search opens the palette); `SearchField` searches in place (find in a transcript).
 */
import { Search } from "lucide-react";
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, Ref } from "react";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { Kbd } from "./Kbd";

export interface SearchButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Keycaps of the shortcut that does the same. */
  keys: readonly string[];
}

export function SearchButton({
  keys,
  className,
  children = "Search",
  type = "button",
  ...props
}: SearchButtonProps) {
  return (
    <button
      type={type}
      className={cn("nt-searchbar", className)}
      aria-keyshortcuts={keys.join("+")}
      {...props}
    >
      <Icon icon={Search} size="sm" />
      <span className="nt-searchbar-text">{children}</span>
      <Kbd keys={keys} />
    </button>
  );
}

export interface SearchFieldProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "size"> {
  /** Keycaps of the shortcut that focuses the field; shown while it is empty. */
  keys: readonly string[];
  /** Replaces the keycaps while there is something to show (a match count, a clear button). */
  trailing?: ReactNode;
  ref?: Ref<HTMLInputElement>;
}

export function SearchField({ keys, trailing, className, ...props }: SearchFieldProps) {
  return (
    <label className={cn("nt-searchbar", className)}>
      <Icon icon={Search} size="sm" />
      <input
        type="search"
        spellCheck={false}
        autoComplete="off"
        aria-keyshortcuts={keys.join("+")}
        {...props}
      />
      {trailing ?? <Kbd keys={keys} />}
    </label>
  );
}
