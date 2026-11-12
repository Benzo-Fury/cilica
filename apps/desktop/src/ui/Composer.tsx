/**
 * The assistant's input (Composer doc): one row holding an optional leading control, a field that
 * grows to 8 lines and the round Send, which stays on the field's last line. While Margin works,
 * Send becomes Stop (the panel's glow under the box shows the work).
 * ↩ sends, ⇧↩ adds a line, Esc stops.
 */
import { ArrowUp } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import {
  type KeyboardEvent,
  type ReactNode,
  type Ref,
  useCallback,
  useLayoutEffect,
  useRef,
} from "react";
import { tweens } from "@/app/motion";
import { Icon } from "./Icon";
import { Tooltip } from "./Tooltip";

export interface ComposerProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  onStop: () => void;
  working: boolean;
  placeholder: string;
  disabled?: boolean;
  /** Left of the field, on its last line: a small icon control. */
  leading?: ReactNode;
  ref?: Ref<HTMLTextAreaElement>;
}

const MAX_LINES = 8;
const LINE = 20;

export function Composer({
  value,
  onChange,
  onSubmit,
  onStop,
  working,
  placeholder,
  disabled,
  leading,
  ref,
}: ComposerProps) {
  const inner = useRef<HTMLTextAreaElement | null>(null);
  const fitHeight = useCallback(() => {
    const el = inner.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = `${Math.min(el.scrollHeight, MAX_LINES * LINE)}px`;
  }, []);
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-measure when the text changes
  useLayoutEffect(fitHeight, [value]);
  // The field can mount before it has its final width (a panel sliding in, the bar opening), and
  // wraps differently once it does: re-measure whenever the width changes.
  useLayoutEffect(() => {
    const el = inner.current;
    if (!el) return;
    let width = el.clientWidth;
    const observer = new ResizeObserver(() => {
      if (el.clientWidth === width) return;
      width = el.clientWidth;
      fitHeight();
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [fitHeight]);

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.nativeEvent.isComposing) return;
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (!working && value.trim()) onSubmit();
    } else if (event.key === "Escape" && working) {
      event.preventDefault();
      event.stopPropagation();
      onStop();
    }
  };

  const canSend = !working && !disabled && value.trim().length > 0;
  return (
    <div className="nt-composer nt-composer-app" data-working={working || undefined}>
      {leading && <span className="nt-composer-lead">{leading}</span>}
      <textarea
        ref={(el) => {
          inner.current = el;
          if (typeof ref === "function") ref(el);
          else if (ref) ref.current = el;
        }}
        rows={1}
        value={value}
        disabled={disabled}
        placeholder={placeholder}
        aria-label="Message Margin"
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
      />
      <AnimatePresence mode="popLayout" initial={false}>
        {working ? (
          <motion.span
            key="stop"
            className="nt-send-slot"
            initial={{ opacity: 0, scale: 0.8 }}
            animate={{ opacity: 1, scale: 1, transition: tweens.base }}
            exit={{ opacity: 0, scale: 0.8, transition: tweens.exit }}
          >
            <Tooltip label="Stop" shortcut={["esc"]} side="top">
              <button
                type="button"
                className="nt-send nt-send-stop"
                aria-label="Stop"
                onClick={onStop}
              >
                <span className="nt-send-square" />
              </button>
            </Tooltip>
          </motion.span>
        ) : (
          <motion.span
            key="send"
            className="nt-send-slot"
            initial={{ opacity: 0, scale: 0.8 }}
            animate={{ opacity: 1, scale: 1, transition: tweens.base }}
            exit={{ opacity: 0, scale: 0.8, transition: tweens.exit }}
          >
            <Tooltip label="Send" shortcut={["↩"]} side="top" enabled={canSend}>
              <button
                type="button"
                className="nt-send nt-anim"
                aria-label="Send"
                disabled={!canSend}
                onClick={onSubmit}
              >
                <Icon icon={ArrowUp} size="sm" motion="up" />
              </button>
            </Tooltip>
          </motion.span>
        )}
      </AnimatePresence>
    </div>
  );
}
