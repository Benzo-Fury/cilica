/**
 * The graph's hover highlight as a set of glows, one per node that is or was just hovered. The
 * hovered node's glow eases in while the ones you left ease out, so sweeping the pointer across
 * many nodes crossfades from one highlight to the next instead of jumping.
 *
 * Hover is a frequent action, so this is a quick tween with no bounce (motion.md: 80–140ms).
 */

/** Time constant of the ease, in seconds: about 98% of the way there after 120ms. */
const TAU = 0.03;
/** Close enough to the target to stop. */
const EPSILON = 0.01;

export interface HoverGlow {
  node: number;
  /** 0 (off) to 1 (fully lit). */
  amount: number;
}

export class HoverFade {
  glows: HoverGlow[] = [];
  /**
   * How strongly the rest of the graph falls back, 0 to 1. It eases on whether anything is
   * hovered, not on the glows, so moving from node to node keeps the graph dimmed throughout.
   */
  dim = 0;

  /** Drops every glow at once (the graph's data changed). */
  reset(): void {
    this.glows = [];
    this.dim = 0;
  }

  /** The glow on `node` itself, 0 if it has none. */
  amountOf(node: number): number {
    for (const g of this.glows) if (g.node === node) return g.amount;
    return 0;
  }

  /**
   * Moves every glow `dt` seconds toward its target: 1 for `hover`, 0 for the rest. Returns true
   * while anything is still moving.
   */
  step(hover: number, dt: number): boolean {
    if (hover >= 0 && !this.glows.some((g) => g.node === hover)) {
      this.glows.push({ node: hover, amount: 0 });
    }
    const t = 1 - Math.exp(-Math.max(0, dt) / TAU);
    const ease = (from: number, to: number) => {
      if (from === to) return to;
      const next = from + (to - from) * t;
      return Math.abs(next - to) < EPSILON ? to : next;
    };
    this.dim = ease(this.dim, hover >= 0 ? 1 : 0);
    let moving = this.dim !== (hover >= 0 ? 1 : 0);
    for (const g of this.glows) {
      const want = g.node === hover ? 1 : 0;
      g.amount = ease(g.amount, want);
      if (g.amount !== want) moving = true;
    }
    this.glows = this.glows.filter((g) => g.amount > 0 || g.node === hover);
    return moving;
  }

  /**
   * How lit each node is: the strongest glow on it or on a neighbour. Fills `out` (length n) and
   * returns it.
   */
  litness(neighbours: readonly (readonly number[])[], out: Float32Array): Float32Array {
    out.fill(0);
    for (const g of this.glows) {
      if (g.amount > (out[g.node] ?? 0)) out[g.node] = g.amount;
      for (const j of neighbours[g.node] ?? []) {
        if (g.amount > (out[j] ?? 0)) out[j] = g.amount;
      }
    }
    return out;
  }
}
