/**
 * One OS permission (PermissionRow doc): icon, title, what Cilica does with it, and its status.
 * A granted row settles to a success chip; the button morphs into it with `bouncy`.
 */
import type { LucideIcon } from "lucide-react";
import { ArrowRight, Check, ExternalLink } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { Button } from "./Button";
import { Chip } from "./Chip";
import { Icon } from "./Icon";
import { Shimmer } from "./Skeleton";
import { Spinner } from "./Spinner";

export type PermissionState = "granted" | "denied" | "not_determined" | "not_needed";

export interface PermissionRowProps {
  icon: LucideIcon;
  title: string;
  description: string;
  status: PermissionState | undefined;
  /** The next missing permission gets the primary Allow; the rest are secondary. */
  primary?: boolean;
  /** The OS prompt is open (it can wait up to two minutes for an answer). */
  busy?: boolean;
  /**
   * For a permission the OS may not confirm until first use (system audio): show this calm
   * note instead of an Allow call to action.
   */
  unconfirmedNote?: string;
  onRequest: () => void;
  onOpenSettings: () => void;
}

export function PermissionRow({
  icon,
  title,
  description,
  status,
  primary = false,
  busy = false,
  unconfirmedNote,
  onRequest,
  onOpenSettings,
}: PermissionRowProps) {
  const reduced = useReducedMotion();
  const enter = {
    initial: { opacity: 0, scale: reduced ? 1 : 0.8 },
    animate: { opacity: 1, scale: 1, transition: reduced ? reducedTransition : springs.bouncy },
    exit: { opacity: 0, scale: reduced ? 1 : 0.9, transition: tweens.exit },
  };
  const key =
    status === "granted" || status === "not_needed" ? "ok" : busy ? "busy" : (status ?? "loading");
  return (
    <div className="nt-perm nt-perm-app">
      <Icon icon={icon} />
      <div className="nt-perm-text">
        <div className="nt-perm-title">{title}</div>
        <div className="nt-perm-desc">{description}</div>
      </div>
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span key={key} className="nt-perm-status" {...enter}>
          {status === "granted" && (
            <Chip tone="success" icon={Check}>
              Allowed
            </Chip>
          )}
          {status === "not_needed" && (
            <Chip tone="success" icon={Check}>
              Not needed
            </Chip>
          )}
          {status === "not_determined" && busy && (
            <span className="nt-perm-waiting">
              <Spinner />
              <Shimmer>Waiting for your answer…</Shimmer>
            </span>
          )}
          {status === "not_determined" && !busy && unconfirmedNote && (
            <span className="nt-perm-waiting">
              <Chip>{unconfirmedNote}</Chip>
              <Button size="sm" variant="ghost" onClick={onRequest}>
                Check
              </Button>
            </span>
          )}
          {status === "not_determined" && !busy && !unconfirmedNote && (
            <Button size="sm" variant={primary ? "primary" : "secondary"} onClick={onRequest}>
              Allow
              <Icon icon={ArrowRight} size="sm" motion="arrow" />
            </Button>
          )}
          {status === "denied" && (
            <Button size="sm" variant="secondary" icon={ExternalLink} onClick={onOpenSettings}>
              Open Settings
            </Button>
          )}
        </motion.span>
      </AnimatePresence>
    </div>
  );
}
