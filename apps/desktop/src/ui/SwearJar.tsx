/**
 * The swear jar (features/swearjar): a glass jar that fills with coins, a coin stack per person,
 * and the small jar that rises from the canvas corner when a coin goes in.
 *
 * `SwearJarPopup` only draws and animates. The feature decides when a coin flies, from where, and
 * what the count is; `fly()` resolves when the coin lands so the count can tick up on the landing.
 */
import { useReducedMotion } from "motion/react";
import {
  type CSSProperties,
  forwardRef,
  type ReactNode,
  useId,
  useImperativeHandle,
  useRef,
} from "react";
import { createPortal } from "react-dom";
import { cn } from "./cn";
import { RollingNumber } from "./RollingNumber";

// ---------- Jar ---------- //

/** The glass body, in the 64×76 jar box. */
const BODY =
  "M18 19 C9 21 6 27 6 35 L6 65 C6 71 10 74 16 74 L48 74 C54 74 58 71 58 65 L58 35 C58 27 55 21 46 19 Z";

/** Where each coin settles, bottom row first, four to a row with a little jitter. */
const PILE: readonly [number, number][] = Array.from({ length: 44 }, (_, i) => {
  const row = Math.floor(i / 4);
  const col = i % 4;
  return [
    12 + col * 13 + (row % 2 ? 6 : 0) + (((i * 37) % 5) - 2) * 0.7,
    69.5 - row * 3 + (((i * 13) % 3) - 1) * 0.4,
  ];
});

export interface JarArtProps {
  count: number;
  /** Print the count on the jar's label. */
  label?: boolean;
  /** An empty, neglected jar. */
  cobwebs?: boolean;
  /** Coins fill in one after another when the jar first shows (Home, Settings). */
  fill?: boolean;
  className?: string;
}

/** A glass jar with a slot in its lid, holding up to 44 visible coins. */
export function JarArt({
  count,
  label = false,
  cobwebs = false,
  fill = false,
  className,
}: JarArtProps) {
  const clip = useId();
  return (
    <svg viewBox="0 0 64 76" className={cn("nt-jar", fill && "nt-jar-fill", className)} aria-hidden>
      <defs>
        <clipPath id={clip}>
          <path d={BODY} />
        </clipPath>
      </defs>
      <rect className="nt-jar-neck" x="18" y="12" width="28" height="8" rx="2" />
      <path className="nt-jar-glass" d={BODY} />
      <g clipPath={`url(#${clip})`}>
        {PILE.slice(0, Math.min(count, PILE.length)).map(([x, y], i) => (
          <ellipse
            // biome-ignore lint/suspicious/noArrayIndexKey: coins only ever add on top
            key={i}
            className="nt-jar-coin"
            cx={x}
            cy={y}
            rx="6.6"
            ry="2.5"
            style={{ "--i": i } as CSSProperties}
          />
        ))}
      </g>
      {cobwebs && (
        <g className="nt-jar-web">
          <path d="M8 36 Q16 38 18 46 M8 36 Q14 44 10 52 M8 42 Q12 42 13 46" />
          <path d="M56 34 Q50 37 49 44 M56 34 Q52 42 56 48" />
        </g>
      )}
      <rect className="nt-jar-label" x="14.5" y="24" width="35" height="11" rx="2.5" />
      {label && (
        <text className="nt-jar-count" x="32" y="29.8">
          {count}
        </text>
      )}
      <rect className="nt-jar-lid" x="14" y="4" width="36" height="9" rx="3" />
      <rect className="nt-jar-slot" x="24" y="7.2" width="16" height="2.6" rx="1.3" />
    </svg>
  );
}

/** A pile of coins for one person; `of` is the biggest count shown beside it. */
export function CoinStack({ count, of }: { count: number; of: number }) {
  if (count === 0) return <span className="nt-coinstack nt-coinstack-empty" aria-hidden />;
  const coins = Math.max(1, Math.round((count / Math.max(1, of)) * 14));
  return (
    <span className="nt-coinstack" aria-hidden>
      {Array.from({ length: coins }, (_, i) => (
        <i
          // biome-ignore lint/suspicious/noArrayIndexKey: a stack of identical coins
          key={i}
          style={{ "--x": `${((i * 7) % 5) - 2}px`, "--i": i } as CSSProperties}
        />
      ))}
    </span>
  );
}

/** A jar on its own: on Home (`lg`) or beside a count in Settings (`md`). Coins fill in. */
export function JarFigure({
  count,
  cobwebs,
  size = "lg",
}: {
  count: number;
  cobwebs?: boolean;
  size?: "md" | "lg";
}) {
  return (
    <span className={cn("nt-jar-figure", `nt-jar-figure-${size}`)}>
      <JarArt count={count} label cobwebs={cobwebs} fill />
    </span>
  );
}

export interface CoinColumn {
  id: string;
  label: string;
  count: number;
}

/** A coin stack per person, with their name and count under it. */
export function CoinStacks({ columns }: { columns: CoinColumn[] }) {
  const of = Math.max(1, ...columns.map((c) => c.count));
  return (
    <div className="nt-coinstacks">
      {columns.map((c) => (
        <div key={c.id} className="nt-coinstacks-col">
          <CoinStack count={c.count} of={of} />
          <span className="nt-coinstacks-name">{c.label}</span>
          <span className="nt-coinstacks-n">{c.count}</span>
        </div>
      ))}
    </div>
  );
}

/** The jar, this week's count and what it means, with one action beside it (Settings). */
export function JarHero({
  count,
  caption,
  action,
}: {
  count: number;
  caption: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="nt-card nt-jar-hero">
      <JarFigure count={count} size="md" />
      <div className="nt-jar-hero-text">
        <span className="nt-jar-hero-count">
          <RollingNumber value={count} />
          <small>this week</small>
        </span>
        <span className="nt-jar-hero-caption">{caption}</span>
      </div>
      {action}
    </div>
  );
}

export interface JarWeek {
  id: string;
  label: string;
  count: number;
  current?: boolean;
  record?: boolean;
}

/** Coins per week as bars, this week in the accent and the record week tinted. */
export function JarWeeks({ weeks }: { weeks: JarWeek[] }) {
  const max = Math.max(1, ...weeks.map((w) => w.count));
  return (
    <div className="nt-jar-weeks">
      {weeks.map((w, i) => (
        <div
          key={w.id}
          className="nt-jar-week"
          data-current={w.current || undefined}
          data-record={w.record || undefined}
          title={`${w.count}`}
        >
          <i style={{ "--f": w.count / max, "--i": i } as CSSProperties} />
          <small>{w.label}</small>
        </div>
      ))}
    </div>
  );
}

/** Wears the Home amber whatever the accent, like the faces: for the jar's numbers in Settings. */
export function JarScope({ children }: { children: ReactNode }) {
  return <div className="nt-jar-scope">{children}</div>;
}

// ---------- Popup ---------- //

export interface SwearJarHandle {
  /** Send a coin from a word on screen (or from just above the jar) into the slot. */
  fly(from: DOMRect | null): Promise<void>;
}

export interface SwearJarPopupProps {
  /** The count on the jar's label. */
  count: number;
  /** Coins in this visit: "+1", "+2"… Zero hides the badge. */
  streak: number;
  shown: boolean;
  /** These coins were said while Cilica was in the background. */
  away: boolean;
}

/** A spring curve from the motion tokens, for the Web Animations API. */
function curve(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

/** The small jar in the canvas's bottom-left corner, and the coins that fly into it. */
export const SwearJarPopup = forwardRef<SwearJarHandle, SwearJarPopupProps>(function SwearJarPopup(
  { count, streak, shown, away },
  ref,
) {
  const reduced = useReducedMotion();
  const host = useRef<HTMLDivElement>(null);
  const art = useRef<HTMLDivElement>(null);
  const layer = useRef<HTMLDivElement>(null);

  useImperativeHandle(
    ref,
    () => ({
      fly(from) {
        return new Promise<void>((resolve) => {
          const jar = host.current;
          const coins = layer.current;
          // Reduce motion: no flight, the count still lands after a beat.
          if (!jar || !coins || reduced) {
            setTimeout(resolve, 140);
            return;
          }

          // The slot where the jar rests, ignoring the slide it may be in the middle of
          const parent = (jar.offsetParent as HTMLElement | null)?.getBoundingClientRect();
          const scale = jar.offsetWidth / 64;
          const slot = {
            x: (parent?.left ?? 0) + jar.offsetLeft + jar.offsetWidth / 2,
            y: (parent?.top ?? 0) + jar.offsetTop + 8 * scale,
          };
          const start = from
            ? { x: from.left + from.width / 2, y: from.top + from.height / 2 }
            : { x: slot.x, y: slot.y - 72 };
          const dx = slot.x - start.x;
          const dy = slot.y - start.y;

          // One coin: the outer element travels across, the inner one arcs and flips
          const outer = document.createElement("div");
          outer.className = "nt-coin-x";
          outer.style.left = `${start.x}px`;
          outer.style.top = `${start.y}px`;
          const inner = document.createElement("div");
          inner.className = "nt-coin-y";
          const coin = document.createElement("div");
          coin.className = "nt-coin";
          inner.append(coin);
          outer.append(inner);
          coins.append(outer);

          let flight: Animation;
          if (from) {
            const ms = Math.min(950, Math.max(640, 520 + Math.hypot(dx, dy) * 0.45));
            outer.animate(
              [
                { transform: "translateX(0)" },
                {
                  transform: "translateX(0)",
                  offset: 0.22,
                  easing: "cubic-bezier(0.35, 0, 0.6, 1)",
                },
                { transform: `translateX(${dx}px)` },
              ],
              { duration: ms, fill: "forwards" },
            );
            flight = inner.animate(
              [
                { transform: "translateY(0) scale(0)", easing: "cubic-bezier(0.2, 1.5, 0.4, 1)" },
                { transform: "translateY(-30px) scale(1.12)", offset: 0.2, easing: "ease-out" },
                {
                  transform: "translateY(-36px) scale(1)",
                  offset: 0.3,
                  easing: "cubic-bezier(0.55, 0, 0.95, 0.6)",
                },
                { transform: `translateY(${dy}px) scale(0.75)`, offset: 0.96, opacity: 1 },
                { transform: `translateY(${dy + 4}px) scale(0.5)`, opacity: 0 },
              ],
              { duration: ms, fill: "forwards" },
            );
          } else {
            flight = inner.animate(
              [
                { transform: "translateY(0) scale(0.9)", opacity: 0 },
                { transform: "translateY(10px) scale(1)", opacity: 1, offset: 0.15 },
                { transform: `translateY(${dy}px) scale(0.6)`, opacity: 1, offset: 0.95 },
                { transform: `translateY(${dy + 3}px) scale(0.5)`, opacity: 0 },
              ],
              { duration: 380, easing: "cubic-bezier(0.55, 0, 1, 0.6)", fill: "forwards" },
            );
          }

          // Clink: the jar squashes as the coin goes in
          flight.onfinish = () => {
            outer.remove();
            art.current?.animate(
              [
                { transform: "scale(1, 1)" },
                { transform: "scale(1.06, 0.93)", offset: 0.3 },
                { transform: "scale(1, 1)" },
              ],
              { duration: 330, easing: curve("--spring-snappy", "ease-out") },
            );
            resolve();
          };
          flight.oncancel = () => {
            outer.remove();
            resolve();
          };
        });
      },
    }),
    [reduced],
  );

  return (
    <>
      {typeof document !== "undefined" &&
        createPortal(<div ref={layer} className="nt-coin-layer" aria-hidden />, document.body)}
      <div ref={host} className="nt-swearjar" data-shown={shown} aria-hidden={!shown}>
        <div ref={art} className="nt-swearjar-art">
          <JarArt count={count} />
        </div>
        <span className="nt-swearjar-label">
          <RollingNumber value={count} />
        </span>
        {streak > 0 && (
          <span key={streak} className="nt-swearjar-plus">
            +{streak}
          </span>
        )}
        {away && <span className="nt-swearjar-away">While you were away</span>}
      </div>
      <span className="nt-sr-only" role="status">
        {shown && streak > 0 ? `${streak} in the swear jar` : ""}
      </span>
    </>
  );
});
