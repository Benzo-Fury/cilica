import type { LucideIcon } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
import { type KeyboardEvent, useId, useRef } from "react";
import { reducedTransition, springs } from "@/app/motion";
import { cn } from "./cn";
import { Icon } from "./Icon";

export interface SegmentOption<V extends string> {
  value: V;
  label: string;
  icon?: LucideIcon;
}

export interface SegmentedControlProps<V extends string> {
  options: readonly SegmentOption<V>[];
  value: V;
  onChange: (value: V) => void;
  /** Accessible name of the group. */
  label: string;
  /** Fixed width per segment (the Notes / Transcript switch uses 112px). */
  segmentWidth?: "auto" | "wide";
  className?: string;
}

/** Two to four views of the same thing. One thumb slides between them (`springs.smooth`). */
export function SegmentedControl<V extends string>({
  options,
  value,
  onChange,
  label,
  segmentWidth = "auto",
  className,
}: SegmentedControlProps<V>) {
  const id = useId();
  const reduced = useReducedMotion();
  const refs = useRef<(HTMLButtonElement | null)[]>([]);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = options.findIndex((o) => o.value === value);
    const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const next = options[(index + step + options.length) % options.length];
    if (!next) return;
    onChange(next.value);
    refs.current[options.indexOf(next)]?.focus();
  };

  return (
    <div
      role="tablist"
      aria-label={label}
      className={cn("nt-seg", segmentWidth === "wide" && "nt-seg-wide", className)}
      onKeyDown={onKeyDown}
    >
      {options.map((option, i) => {
        const selected = option.value === value;
        return (
          <button
            key={option.value}
            ref={(el) => {
              refs.current[i] = el;
            }}
            type="button"
            role="tab"
            aria-selected={selected}
            tabIndex={selected ? 0 : -1}
            onClick={() => onChange(option.value)}
          >
            {selected && (
              <motion.span
                layoutId={`seg-${id}`}
                className="nt-seg-pill"
                transition={reduced ? reducedTransition : springs.smooth}
                aria-hidden
              />
            )}
            {option.icon && <Icon icon={option.icon} size="sm" />}
            <span>{option.label}</span>
          </button>
        );
      })}
    </div>
  );
}
