/**
 * The graph's "grow" replay: nodes appear one step at a time, in the order the caller gives
 * (for the full graph, when each note was created), and each pops in on a spring.
 *
 * The clock runs on frame time, so the replay takes as long at any refresh rate. A small graph
 * takes about a third of a second per step; a big one is capped so the whole replay stays short,
 * and several steps can land in one frame.
 */

/** Seconds per step when there are few of them. */
const STEP = 0.35;
/** The whole replay never runs shorter or longer than this, in seconds. */
const MIN_TOTAL = 2.5;
const MAX_TOTAL = 12;
/** A beat before the first step, so the empty canvas reads as the start. */
const LEAD_IN = 0.25;

/** The bouncy spring preset (motion.md): visualDuration 0.35s, bounce 0.3. */
const SPRING_DURATION = 0.35;
const SPRING_BOUNCE = 0.3;
/** After this long a node has settled; nothing left to animate. */
export const APPEAR_SETTLE = 0.8;

export class GraphGrowth {
  /** Step at which each node appears; `steps` (or more) means never during this replay. */
  readonly ranks: Int32Array;
  readonly steps: number;
  readonly total: number;
  /** Steps already shown: nodes whose rank is below this are on screen. */
  shown = 0;
  private elapsed = 0;

  constructor(ranks: ArrayLike<number>) {
    this.ranks = Int32Array.from(ranks);
    let max = -1;
    for (const r of this.ranks) if (r > max) max = r;
    this.steps = max + 1;
    this.total = Math.min(MAX_TOTAL, Math.max(MIN_TOTAL, this.steps * STEP));
  }

  get done(): boolean {
    return this.shown >= this.steps;
  }

  /**
   * Moves the clock on by `dt` seconds. Returns the steps that came into view this frame, as
   * `[from, to)`, empty when none did.
   */
  advance(dt: number): [number, number] {
    const from = this.shown;
    if (this.done) return [from, from];
    this.elapsed += Math.max(0, dt);
    const t = (this.elapsed - LEAD_IN) / this.total;
    // The first step shows as soon as the lead-in ends; the last one as the replay ends.
    const want = t < 0 ? 0 : Math.min(this.steps, Math.floor(t * (this.steps - 1)) + 1);
    if (want > from) this.shown = want;
    return [from, this.shown];
  }

  /** Shows every step at once (the replay was stopped). Returns the steps that came into view. */
  finish(): [number, number] {
    const from = this.shown;
    this.shown = this.steps;
    return [from, this.shown];
  }
}

/**
 * How far a node that appeared `t` seconds ago has grown: 0 at first, overshooting a little past
 * 1, then settling on 1 (an underdamped spring, the bouncy preset).
 */
export function appearScale(t: number): number {
  if (t <= 0) return 0;
  if (t >= APPEAR_SETTLE) return 1;
  const omega = (2 * Math.PI) / SPRING_DURATION;
  const zeta = 1 - SPRING_BOUNCE;
  const damped = omega * Math.sqrt(1 - zeta * zeta);
  const decay = Math.exp(-zeta * omega * t);
  return 1 - decay * (Math.cos(damped * t) + ((zeta * omega) / damped) * Math.sin(damped * t));
}

/** A node's opacity as it appears: a quick fade, no bounce. */
export function appearOpacity(t: number): number {
  return Math.max(0, Math.min(1, t / 0.14));
}
