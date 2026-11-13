/**
 * The jar on screen: what its label says, whether it is up, and the coins on their way in.
 *
 * The backend owns the real count (meetings are read from transcripts whether or not the window is
 * open). The label shows `seen`: the coins you have watched go in. When the backend's count runs
 * ahead of `seen` (swears said while Cilica was in the background), `reconcile` drops the
 * difference in as a catch-up the next time you look.
 */
import { create } from "zustand";
import type { SwearJarHandle } from "@/ui";

/** At most this many coins fly in a catch-up; the badge still shows the full number. */
export const CATCH_UP_COINS = 5;
/** How long the jar stays up after the last coin lands. */
const HOLD_MS = 1600;
/** A catch-up stays a little longer, so "While you were away" can be read. */
const AWAY_HOLD_MS = 2600;
/** Coins from several words leave one after another. */
const STAGGER_MS = 140;
/** How long a swear stays washed in amber: it holds while the coin leaves, then fades (ui.css). */
export const WASH_MS = 2200;

interface JarState {
  /** The count on the jar's label. Null until the first count arrives. */
  seen: number | null;
  /** Coins on their way in, not yet on the label. */
  owed: number;
  shown: boolean;
  /** Coins this visit: the "+n" badge. */
  streak: number;
  away: boolean;
  /** Transcript segments whose swears are washed in amber right now. */
  lit: ReadonlySet<string>;
}

export const useJar = create<JarState>()(() => ({
  seen: null,
  owed: 0,
  shown: false,
  streak: 0,
  away: false,
  lit: new Set(),
}));

let handle: SwearJarHandle | null = null;
let hideTimer: ReturnType<typeof setTimeout> | undefined;
let inFlight = 0;

/** The jar on screen, or null when it is not mounted (switched off, or the pop-up is off). */
export function registerJar(next: SwearJarHandle | null) {
  handle = next;
}

/** Wash a segment's swears in amber for a moment; the wash fades and goes after `WASH_MS`. */
export function light(segmentId: string) {
  const { lit } = useJar.getState();
  if (lit.has(segmentId)) return;
  useJar.setState({ lit: new Set([...lit, segmentId]) });
  setTimeout(() => {
    const next = new Set(useJar.getState().lit);
    next.delete(segmentId);
    useJar.setState({ lit: next });
  }, WASH_MS);
}

export interface DropOptions {
  /** Said while Cilica was in the background. */
  away?: boolean;
  /** Called as each coin lands (the clink). */
  onLand?: () => void;
}

/**
 * Put `total` coins in the jar. One coin flies from each origin (a word on screen, or null to drop
 * in from above); a catch-up passes fewer origins than coins and the last one carries the rest.
 */
export function dropCoins(
  origins: readonly (DOMRect | null)[],
  total = origins.length,
  options: DropOptions = {},
) {
  if (total <= 0) return;
  const start = useJar.getState();

  // No jar on screen: the label just catches up
  if (!handle || origins.length === 0) {
    useJar.setState({ seen: (start.seen ?? 0) + total });
    return;
  }

  // Bring the jar up and owe the coins until they land
  clearTimeout(hideTimer);
  useJar.setState({
    shown: true,
    away: start.away || Boolean(options.away),
    owed: start.owed + total,
  });
  let left = total;
  origins.forEach((from, i) => {
    const share = i === origins.length - 1 ? left : 1;
    left -= share;
    inFlight += 1;
    setTimeout(() => {
      const fly = handle ? handle.fly(from) : Promise.resolve();
      void fly.then(() => {
        inFlight -= 1;
        const now = useJar.getState();
        useJar.setState({
          seen: (now.seen ?? 0) + share,
          owed: Math.max(0, now.owed - share),
          streak: now.streak + share,
        });
        options.onLand?.();
        hideLater(options.away ? AWAY_HOLD_MS : HOLD_MS);
      });
    }, i * STAGGER_MS);
  });
}

/** Slide the jar away once nothing is in the air, then reset the badge. */
function hideLater(ms: number) {
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => {
    if (inFlight > 0) return hideLater(400);
    useJar.setState({ shown: false });
    hideTimer = setTimeout(() => useJar.setState({ streak: 0, away: false }), 300);
  }, ms);
}

/**
 * Line the label up with the backend's `count`. More coins than seen drop in as a catch-up
 * (`animate`), fewer (the week turned over, the jar was emptied, a setting changed) just reset.
 */
export function reconcile(count: number, { animate, away }: { animate: boolean; away: boolean }) {
  const { seen, owed } = useJar.getState();
  if (seen === null) {
    useJar.setState({ seen: count });
    return;
  }
  const diff = count - seen - owed;
  if (diff === 0) return;
  if (diff < 0 || !animate) {
    useJar.setState({ seen: Math.max(0, count - owed) });
    return;
  }
  const origins = Array.from({ length: Math.min(diff, CATCH_UP_COINS) }, () => null);
  dropCoins(origins, diff, { away });
}

/** Forget everything (tests). */
export function resetJar() {
  clearTimeout(hideTimer);
  handle = null;
  inFlight = 0;
  useJar.setState({ seen: null, owed: 0, shown: false, streak: 0, away: false, lit: new Set() });
}
