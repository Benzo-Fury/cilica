import clsx from "clsx";
import { Bell, Check, CircleAlert, Undo2, X } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { reducedTransition, springs, tweens } from "@/app/motion";
import type { ToastRecord, ToastStore } from "./store";
import { toastStore, useToasts } from "./toast";
import "./toast.css";

/** Gap between toasts when the stack fans out. */
const GAP = 8;
/** Collapsed stack: each older toast sits 8px lower and 4% smaller. */
const PEEK = 8;
const SHRINK = 0.04;
const BEHIND_OPACITY = [1, 0.85, 0.6];
/** How long a toast stays a pill before it grows into a card. */
const MORPH_DELAY = 250;
const SWIPE_DISTANCE = 80;
const SWIPE_VELOCITY = 400;

/** Reads a px length token from :root, so radii come from tokens.css. */
function tokenPx(name: string, fallback: number): number {
  if (typeof window === "undefined") return fallback;
  const value = Number.parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue(name),
  );
  return Number.isFinite(value) ? value : fallback;
}

function useRadii() {
  return useMemo(() => {
    const sq = tokenPx("--sq", 1) || 1;
    return {
      card: tokenPx("--radius-lg", 14) * sq,
      pill: tokenPx("--size-control-lg", 40) / 2,
    };
  }, []);
}

function ToastIcon({ toast }: { toast: ToastRecord }) {
  if (toast.icon) return <span className="nt-toast-icon">{toast.icon}</span>;
  const icon: Record<ToastRecord["variant"], ReactNode> = {
    default: null,
    success: <Check className="nt-ico" strokeWidth={1.75} aria-hidden />,
    undo: <Check className="nt-ico" strokeWidth={1.75} aria-hidden />,
    error: <CircleAlert className="nt-ico" strokeWidth={1.75} aria-hidden />,
    action: <Bell className="nt-ico nt-i-bell" strokeWidth={1.75} aria-hidden />,
    loading: <span className="nt-toast-spinner" aria-hidden />,
  };
  const glyph = icon[toast.variant];
  if (!glyph) return null;
  return (
    <AnimatePresence mode="popLayout" initial={false}>
      <motion.span
        key={toast.variant}
        className="nt-toast-icon"
        data-variant={toast.variant}
        initial={{ opacity: 0, scale: 0.6 }}
        animate={{ opacity: 1, scale: 1, transition: springs.snappy }}
        exit={{ opacity: 0, scale: 0.6, transition: tweens.exit }}
      >
        {glyph}
      </motion.span>
    </AnimatePresence>
  );
}

interface CardProps {
  store: ToastStore;
  toast: ToastRecord;
  reduced: boolean;
  radii: { card: number; pill: number };
  onHeight: (id: string, height: number) => void;
}

function ToastCard({ store, toast, reduced, radii, onHeight }: CardProps) {
  const ref = useRef<HTMLDivElement>(null);
  const hasExtra = Boolean(toast.description || toast.action || toast.variant === "undo");
  const [expanded, setExpanded] = useState(reduced && hasExtra);

  // The signature morph: every toast lands as a pill, then grows into a card if it has more to say.
  useEffect(() => {
    if (!hasExtra) {
      setExpanded(false);
      return;
    }
    if (reduced) {
      setExpanded(true);
      return;
    }
    const handle = window.setTimeout(() => setExpanded(true), MORPH_DELAY);
    return () => window.clearTimeout(handle);
  }, [hasExtra, reduced]);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    onHeight(toast.id, el.offsetHeight);
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => onHeight(toast.id, el.offsetHeight));
    observer.observe(el);
    return () => observer.disconnect();
  }, [toast.id, onHeight]);

  const runAction = () => {
    toast.action?.onClick();
    store.dismiss(toast.id);
  };
  const runUndo = () => {
    toast.onUndo?.();
    store.dismiss(toast.id);
  };

  const isError = toast.variant === "error";

  return (
    <motion.div
      ref={ref}
      layout={!reduced}
      layoutDependency={`${expanded}:${toast.version}`}
      transition={{ layout: springs.bouncy }}
      style={{ borderRadius: expanded ? radii.card : radii.pill }}
      className={clsx("nt-toast-card nt-glass-strong", expanded ? "is-card" : "is-pill")}
      data-variant={toast.variant}
      role={isError ? "alert" : "status"}
      aria-live={isError ? "assertive" : "polite"}
      aria-atomic="true"
      drag="x"
      dragConstraints={{ left: 0, right: 0 }}
      dragElastic={{ left: 0, right: 1 }}
      dragTransition={{ bounceStiffness: 500, bounceDamping: 32 }}
      onDragEnd={(_, info) => {
        if (info.offset.x > SWIPE_DISTANCE || info.velocity.x > SWIPE_VELOCITY) {
          store.dismiss(toast.id);
        }
      }}
    >
      <motion.div layout={reduced ? false : "position"} className="nt-toast-row">
        <ToastIcon toast={toast} />
        <div className="nt-toast-body">
          <div className="nt-toast-title">{toast.title}</div>
          {expanded && toast.description ? (
            <motion.div
              className="nt-toast-desc"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { ...tweens.base, delay: reduced ? 0 : 0.08 } }}
            >
              {toast.description}
            </motion.div>
          ) : null}
        </div>
        {expanded && toast.action ? (
          <motion.button
            type="button"
            className="nt-btn nt-btn-ghost nt-btn-sm nt-anim nt-toast-action"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: tweens.base }}
            onClick={runAction}
          >
            {toast.action.label}
          </motion.button>
        ) : null}
        {expanded && toast.variant === "undo" ? (
          <motion.span
            className="nt-toast-undo"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: tweens.base }}
          >
            <button
              type="button"
              className="nt-btn nt-btn-ghost nt-btn-sm nt-anim nt-toast-action"
              onClick={runUndo}
            >
              <Undo2 className="nt-ico nt-ico-sm nt-i-undo" strokeWidth={1.75} aria-hidden />
              Undo
            </button>
          </motion.span>
        ) : null}
      </motion.div>
      <button
        type="button"
        className="nt-iconbtn nt-toast-dismiss"
        aria-label="Dismiss notification"
        onClick={() => store.dismiss(toast.id)}
      >
        <X className="nt-ico nt-ico-sm" strokeWidth={1.75} aria-hidden />
      </button>
    </motion.div>
  );
}

/** Renders the toast stack. Mount once, near the root. */
export function Toaster({ store = toastStore }: { store?: ToastStore }) {
  const { toasts } = useToasts(store);
  const reduced = useReducedMotion() ?? false;
  const radii = useRadii();
  const [hovered, setHovered] = useState(false);
  const [heights, setHeights] = useState<Record<string, number>>({});
  const leaveTimer = useRef<number | undefined>(undefined);

  // Newest first: index 0 is the front of the stack.
  const ordered = useMemo(() => toasts.filter((t) => t.status === "visible").reverse(), [toasts]);

  const onHeight = useCallback((id: string, height: number) => {
    setHeights((prev) => (prev[id] === height ? prev : { ...prev, [id]: height }));
  }, []);

  // Timers pause while the window is in the background.
  useEffect(() => {
    const blur = () => store.pause("blur");
    const focus = () => store.resume("blur");
    window.addEventListener("blur", blur);
    window.addEventListener("focus", focus);
    return () => {
      window.removeEventListener("blur", blur);
      window.removeEventListener("focus", focus);
      store.resume("blur");
    };
  }, [store]);

  // When the last toast leaves under the pointer, mouseleave never fires: release the hover pause.
  useEffect(() => {
    if (ordered.length === 0 && hovered) {
      setHovered(false);
      store.resume("hover");
    }
  }, [ordered.length, hovered, store]);

  useEffect(() => () => window.clearTimeout(leaveTimer.current), []);

  const enter = () => {
    window.clearTimeout(leaveTimer.current);
    if (!hovered) {
      setHovered(true);
      store.pause("hover");
    }
  };
  // A short grace period so moving across the gaps of a fanned stack does not collapse it.
  const leave = () => {
    window.clearTimeout(leaveTimer.current);
    leaveTimer.current = window.setTimeout(() => {
      setHovered(false);
      store.resume("hover");
    }, 120);
  };

  const frontHeight = (ordered[0] && heights[ordered[0].id]) ?? 0;
  let fanOffset = 0;
  const placements = ordered.map((t, i) => {
    const height = heights[t.id] ?? frontHeight;
    const fanned = { y: fanOffset, scale: 1, opacity: 1 };
    fanOffset += height + GAP;
    const scale = 1 - SHRINK * i;
    // Collapsed: line each older toast's bottom up 8px below the one in front of it.
    const y = i === 0 ? 0 : Math.max(frontHeight - height * scale, 0) + PEEK * i;
    const collapsed = { y, scale, opacity: BEHIND_OPACITY[i] ?? 0 };
    return hovered ? fanned : collapsed;
  });

  return (
    <section className="nt-toaster" aria-label="Notifications">
      <ol className="nt-toaster-list">
        <AnimatePresence initial={false}>
          {ordered.map((t, i) => {
            const target = placements[i] ?? { y: 0, scale: 1, opacity: 1 };
            return (
              <motion.li
                key={t.id}
                className="nt-toaster-item"
                style={{ zIndex: ordered.length - i }}
                initial={
                  reduced
                    ? { opacity: 0, y: target.y, scale: target.scale }
                    : { opacity: 0, y: -24, scale: 0.9 }
                }
                animate={{
                  ...target,
                  transition: reduced
                    ? { ...reducedTransition, y: { duration: 0 }, scale: { duration: 0 } }
                    : hovered
                      ? springs.smooth
                      : springs.bouncy,
                }}
                exit={{
                  opacity: 0,
                  y: reduced ? target.y : target.y - 8,
                  transition: reduced ? reducedTransition : tweens.exit,
                }}
                onPointerEnter={enter}
                onPointerLeave={leave}
                onFocus={enter}
                onBlur={leave}
              >
                <ToastCard
                  store={store}
                  toast={t}
                  reduced={reduced}
                  radii={radii}
                  onHeight={onHeight}
                />
              </motion.li>
            );
          })}
        </AnimatePresence>
      </ol>
    </section>
  );
}
