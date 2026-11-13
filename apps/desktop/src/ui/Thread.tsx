/**
 * Assistant thread pieces (AssistantThread doc): your turns as `fill-press` bubbles, Cilica's as
 * plain text, tool steps as single `ink-3` lines that expand, and the shimmering working label.
 */
import type { LucideIcon } from "lucide-react";
import { ChevronRight } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
import { type ReactNode, useId, useState } from "react";
import { reducedTransition, springs } from "@/app/motion";
import { Collapse } from "./Collapse";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { Shimmer } from "./Skeleton";

/** Messages enter with a short rise (`snappy`); they never bounce. */
export function ThreadItem({ children, className }: { children: ReactNode; className?: string }) {
  const reduced = useReducedMotion();
  return (
    <motion.div
      className={className}
      initial={{ opacity: 0, y: reduced ? 0 : 6 }}
      animate={{ opacity: 1, y: 0, transition: reduced ? reducedTransition : springs.snappy }}
    >
      {children}
    </motion.div>
  );
}

export function Bubble({ children }: { children: ReactNode }) {
  return <div className="nt-bubble nt-selectable">{children}</div>;
}

export function AiText({ children }: { children: ReactNode }) {
  return <div className="nt-ai nt-ai-app nt-selectable">{children}</div>;
}

/** A one-line tool step ("Searched 14 notes") that opens to show its details. */
export function ToolStep({
  icon,
  label,
  working = false,
  failed = false,
  details,
}: {
  icon: LucideIcon;
  label: string;
  working?: boolean;
  failed?: boolean;
  details?: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  if (working) {
    return (
      <div className="nt-step">
        <Icon icon={icon} size="sm" />
        <Shimmer>{label}</Shimmer>
      </div>
    );
  }
  return (
    <div className={cn("nt-step-wrap", failed && "nt-step-failed")}>
      <div className="nt-step">
        <button
          type="button"
          aria-expanded={details ? open : undefined}
          aria-controls={details ? id : undefined}
          disabled={!details}
          onClick={() => setOpen((o) => !o)}
        >
          <Icon icon={icon} size="sm" />
          <span>{label}</span>
          {details && (
            <ChevronRight className="nt-ico nt-ico-sm nt-chevron" strokeWidth={1.75} aria-hidden />
          )}
        </button>
      </div>
      {details && (
        <Collapse open={open} id={id}>
          <div className="nt-step-details">{details}</div>
        </Collapse>
      )}
    </div>
  );
}

/** An error line in the thread: what went wrong and, optionally, an action that fixes it. */
export function ThreadNotice({
  icon,
  children,
  tone = "neutral",
  action,
}: {
  icon: LucideIcon;
  children: ReactNode;
  tone?: "neutral" | "danger";
  /** Shown on its own line under the message, so it never wraps into the sentence. */
  action?: ReactNode;
}) {
  return (
    <div
      role={tone === "danger" ? "alert" : undefined}
      className={cn("nt-thread-notice", tone === "danger" && "nt-thread-notice-danger")}
    >
      <Icon icon={icon} size="sm" />
      <div className="nt-thread-notice-body">
        <p>{children}</p>
        {action && <div className="nt-thread-notice-action">{action}</div>}
      </div>
    </div>
  );
}
