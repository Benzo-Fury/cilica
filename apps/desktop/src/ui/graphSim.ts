/**
 * A small force-directed layout for the link graph (the same model as d3-force, which Obsidian's
 * graph also follows): springs along edges, repulsion between every pair of nodes (Barnes–Hut
 * approximated past a couple of hundred nodes, so it stays O(n log n)), and a gentle pull to the
 * centre. Typed arrays, no allocation per tick. It cools down and stops: callers stop ticking once
 * `settled` is true, so an idle graph costs nothing.
 */

export interface SimOptions {
  /** Rest length of an edge, in layout units. */
  linkDistance: number;
  /** Repulsion per node (negative pushes apart). */
  charge: number;
  /** Pull towards the origin, 0–1. */
  gravity: number;
}

const ALPHA_MIN = 0.003;
const ALPHA_DECAY = 1 - ALPHA_MIN ** (1 / 280);
const VELOCITY_DECAY = 0.42;
const THETA2 = 0.81;
const DISTANCE_MIN2 = 1;
/** Above this many nodes repulsion uses the quadtree. */
const EXACT_LIMIT = 220;

export class ForceSim {
  readonly n: number;
  readonly x: Float64Array;
  readonly y: Float64Array;
  readonly vx: Float64Array;
  readonly vy: Float64Array;
  /** 1 while a node is held (dragged): it stays where it is put. */
  readonly pinned: Uint8Array;
  /**
   * 1 for nodes in the layout. A 0 node feels no force, pushes nothing and stays where it is
   * (the graph's "grow" replay adds nodes one by one). All 1 unless a caller changes it.
   */
  readonly active: Uint8Array;
  private readonly src: Uint32Array;
  private readonly dst: Uint32Array;
  private readonly strength: Float64Array;
  private readonly bias: Float64Array;
  private readonly charges: Float64Array;
  private readonly tree: QuadTree;
  alpha = 1;
  alphaTarget = 0;
  options: SimOptions;

  constructor(
    n: number,
    edges: readonly { source: number; target: number }[],
    options: SimOptions,
    initial?: (i: number) => { x: number; y: number } | null,
  ) {
    this.n = n;
    this.options = options;
    this.x = new Float64Array(n);
    this.y = new Float64Array(n);
    this.vx = new Float64Array(n);
    this.vy = new Float64Array(n);
    this.pinned = new Uint8Array(n);
    this.active = new Uint8Array(n).fill(1);
    const valid = edges.filter((e) => e.source < n && e.target < n && e.source !== e.target);
    this.src = Uint32Array.from(valid, (e) => e.source);
    this.dst = Uint32Array.from(valid, (e) => e.target);
    const degree = new Uint32Array(n);
    for (const e of valid) {
      degree[e.source] = (degree[e.source] ?? 0) + 1;
      degree[e.target] = (degree[e.target] ?? 0) + 1;
    }
    this.strength = Float64Array.from(
      valid,
      (e) => 1 / Math.min(degree[e.source] ?? 1, degree[e.target] ?? 1),
    );
    this.bias = Float64Array.from(valid, (e) => {
      const s = degree[e.source] ?? 1;
      return s / (s + (degree[e.target] ?? 1));
    });
    // Hubs push a little harder, so their neighbours fan out around them.
    this.charges = Float64Array.from(
      { length: n },
      (_, i) => options.charge * (1 + Math.sqrt(degree[i] ?? 0) * 0.15),
    );
    this.tree = new QuadTree(n);
    // Phyllotaxis start (as d3), or where the caller remembers the node.
    const step = Math.PI * (3 - Math.sqrt(5));
    for (let i = 0; i < n; i++) {
      const known = initial?.(i);
      if (known) {
        this.x[i] = known.x;
        this.y[i] = known.y;
      } else {
        const r = options.linkDistance * 0.35 * Math.sqrt(0.5 + i);
        this.x[i] = r * Math.cos(i * step);
        this.y[i] = r * Math.sin(i * step);
      }
    }
  }

  get settled(): boolean {
    return this.alpha < ALPHA_MIN && this.alphaTarget === 0;
  }

  /** Warm up: run more energy into the layout (a drag, new data). */
  reheat(alpha = 0.3) {
    this.alpha = Math.max(this.alpha, alpha);
  }

  tick(): void {
    const { n, x, y, vx, vy } = this;
    this.alpha += (this.alphaTarget - this.alpha) * ALPHA_DECAY;
    const alpha = this.alpha;

    // Springs along edges.
    const dist = this.options.linkDistance;
    for (let k = 0; k < this.src.length; k++) {
      const s = this.src[k] as number;
      const t = this.dst[k] as number;
      if (!this.active[s] || !this.active[t]) continue;
      let dx = (x[t] as number) + (vx[t] as number) - (x[s] as number) - (vx[s] as number);
      let dy = (y[t] as number) + (vy[t] as number) - (y[s] as number) - (vy[s] as number);
      if (dx === 0 && dy === 0) dx = jiggle();
      let l = Math.sqrt(dx * dx + dy * dy);
      l = ((l - dist) / l) * alpha * (this.strength[k] as number);
      dx *= l;
      dy *= l;
      const b = this.bias[k] as number;
      vx[t] = (vx[t] as number) - dx * b;
      vy[t] = (vy[t] as number) - dy * b;
      vx[s] = (vx[s] as number) + dx * (1 - b);
      vy[s] = (vy[s] as number) + dy * (1 - b);
    }

    // Repulsion between nodes.
    if (n <= EXACT_LIMIT) {
      for (let i = 0; i < n; i++) {
        if (!this.active[i]) continue;
        for (let j = i + 1; j < n; j++) {
          if (!this.active[j]) continue;
          let dx = (x[j] as number) - (x[i] as number);
          let dy = (y[j] as number) - (y[i] as number);
          if (dx === 0 && dy === 0) {
            dx = jiggle();
            dy = jiggle();
          }
          const l = Math.max(dx * dx + dy * dy, DISTANCE_MIN2);
          const wi = ((this.charges[j] as number) * alpha) / l;
          const wj = ((this.charges[i] as number) * alpha) / l;
          vx[i] = (vx[i] as number) + dx * wi;
          vy[i] = (vy[i] as number) + dy * wi;
          vx[j] = (vx[j] as number) - dx * wj;
          vy[j] = (vy[j] as number) - dy * wj;
        }
      }
    } else {
      this.tree.build(x, y, this.charges, n, this.active);
      for (let i = 0; i < n; i++) if (this.active[i]) this.tree.apply(i, x, y, vx, vy, alpha);
    }

    // Pull to the centre, then move.
    const g = this.options.gravity * alpha;
    for (let i = 0; i < n; i++) {
      if (this.pinned[i] || !this.active[i]) {
        vx[i] = 0;
        vy[i] = 0;
        continue;
      }
      vx[i] = ((vx[i] as number) - (x[i] as number) * g) * (1 - VELOCITY_DECAY);
      vy[i] = ((vy[i] as number) - (y[i] as number) * g) * (1 - VELOCITY_DECAY);
      x[i] = (x[i] as number) + (vx[i] as number);
      y[i] = (y[i] as number) + (vy[i] as number);
    }
  }
}

function jiggle(): number {
  return (Math.random() - 0.5) * 1e-6;
}

/** Barnes–Hut quadtree in flat arrays, rebuilt each tick into the same buffers. */
class QuadTree {
  private cap: number;
  private size = 0;
  private x0!: Float64Array;
  private y0!: Float64Array;
  private w!: Float64Array;
  private child!: Int32Array;
  private point!: Int32Array;
  private mass!: Float64Array;
  private cx!: Float64Array;
  private cy!: Float64Array;
  private readonly stack: Int32Array;

  constructor(n: number) {
    this.cap = Math.max(16, n * 4);
    this.alloc(this.cap);
    this.stack = new Int32Array(Math.max(64, n * 4 + 64));
  }

  private alloc(cap: number) {
    const grow = <T extends Float64Array | Int32Array>(
      old: T | undefined,
      make: (c: number) => T,
      per = 1,
    ): T => {
      const next = make(cap * per);
      if (old) next.set(old.subarray(0, Math.min(old.length, next.length)) as never);
      return next;
    };
    this.x0 = grow(this.x0, (c) => new Float64Array(c));
    this.y0 = grow(this.y0, (c) => new Float64Array(c));
    this.w = grow(this.w, (c) => new Float64Array(c));
    this.child = grow(this.child, (c) => new Int32Array(c), 4);
    this.point = grow(this.point, (c) => new Int32Array(c));
    this.mass = grow(this.mass, (c) => new Float64Array(c));
    this.cx = grow(this.cx, (c) => new Float64Array(c));
    this.cy = grow(this.cy, (c) => new Float64Array(c));
  }

  private node(x0: number, y0: number, w: number): number {
    if (this.size >= this.cap) {
      this.cap *= 2;
      this.alloc(this.cap);
    }
    const q = this.size++;
    this.x0[q] = x0;
    this.y0[q] = y0;
    this.w[q] = w;
    this.child.fill(-1, q * 4, q * 4 + 4);
    this.point[q] = -1;
    this.mass[q] = 0;
    this.cx[q] = 0;
    this.cy[q] = 0;
    return q;
  }

  build(x: Float64Array, y: Float64Array, charges: Float64Array, n: number, active: Uint8Array) {
    let minX = Number.POSITIVE_INFINITY;
    let minY = Number.POSITIVE_INFINITY;
    let maxX = Number.NEGATIVE_INFINITY;
    let maxY = Number.NEGATIVE_INFINITY;
    for (let i = 0; i < n; i++) {
      if (!active[i]) continue;
      const xi = x[i] as number;
      const yi = y[i] as number;
      if (xi < minX) minX = xi;
      if (xi > maxX) maxX = xi;
      if (yi < minY) minY = yi;
      if (yi > maxY) maxY = yi;
    }
    if (minX > maxX) {
      minX = 0;
      minY = 0;
    }
    const w = Math.max(maxX - minX, maxY - minY, 1) * 1.0001;
    this.size = 0;
    this.node(minX, minY, w);
    for (let i = 0; i < n; i++) if (active[i]) this.insert(i, x, y);
    this.accumulate(x, y, charges);
  }

  private insert(i: number, x: Float64Array, y: Float64Array) {
    let q = 0;
    for (let depth = 0; depth < 48; depth++) {
      const p = this.point[q] as number;
      const kids = q * 4;
      const isLeaf =
        this.child[kids] === -1 &&
        this.child[kids + 1] === -1 &&
        this.child[kids + 2] === -1 &&
        this.child[kids + 3] === -1;
      if (isLeaf && p === -1) {
        this.point[q] = i;
        return;
      }
      if (isLeaf && p >= 0) {
        // Split: push the resident point down, then keep going.
        this.point[q] = -1;
        if (x[p] === x[i] && y[p] === y[i]) {
          x[i] = (x[i] as number) + jiggle() * 1e4;
          y[i] = (y[i] as number) + jiggle() * 1e4;
        }
        const pq = this.quadrant(q, x[p] as number, y[p] as number);
        this.point[pq] = p;
      }
      q = this.quadrant(q, x[i] as number, y[i] as number);
    }
    // Pathological overlap: drop into this cell's slot (it still gets the centre-of-mass force).
    if (this.point[q] === -1) this.point[q] = i;
  }

  /** The child cell of `q` holding (px, py), created if needed. */
  private quadrant(q: number, px: number, py: number): number {
    const half = (this.w[q] as number) / 2;
    const right = px >= (this.x0[q] as number) + half ? 1 : 0;
    const bottom = py >= (this.y0[q] as number) + half ? 1 : 0;
    const slot = q * 4 + bottom * 2 + right;
    let c = this.child[slot] as number;
    if (c === -1) {
      c = this.node(
        (this.x0[q] as number) + right * half,
        (this.y0[q] as number) + bottom * half,
        half,
      );
      this.child[slot] = c;
    }
    return c;
  }

  /** Charge and centre of charge per cell, children before parents (cells are created after their parents). */
  private accumulate(x: Float64Array, y: Float64Array, charges: Float64Array) {
    for (let q = this.size - 1; q >= 0; q--) {
      const p = this.point[q] as number;
      let m = 0;
      let sx = 0;
      let sy = 0;
      if (p >= 0) {
        const c = charges[p] as number;
        m += c;
        sx += c * (x[p] as number);
        sy += c * (y[p] as number);
      }
      for (let k = 0; k < 4; k++) {
        const c = this.child[q * 4 + k] as number;
        if (c === -1) continue;
        const cm = this.mass[c] as number;
        m += cm;
        sx += cm * (this.cx[c] as number);
        sy += cm * (this.cy[c] as number);
      }
      this.mass[q] = m;
      this.cx[q] = m !== 0 ? sx / m : 0;
      this.cy[q] = m !== 0 ? sy / m : 0;
    }
  }

  apply(
    i: number,
    x: Float64Array,
    y: Float64Array,
    vx: Float64Array,
    vy: Float64Array,
    alpha: number,
  ) {
    const xi = x[i] as number;
    const yi = y[i] as number;
    let top = 0;
    this.stack[top++] = 0;
    let fx = 0;
    let fy = 0;
    while (top > 0) {
      const q = this.stack[--top] as number;
      const m = this.mass[q] as number;
      if (m === 0) continue;
      const dx = (this.cx[q] as number) - xi;
      const dy = (this.cy[q] as number) - yi;
      const w = this.w[q] as number;
      const l = dx * dx + dy * dy;
      const p = this.point[q] as number;
      const kids = q * 4;
      const isLeaf =
        this.child[kids] === -1 &&
        this.child[kids + 1] === -1 &&
        this.child[kids + 2] === -1 &&
        this.child[kids + 3] === -1;
      if (!isLeaf && (w * w) / THETA2 < l) {
        // Far enough: the whole cell acts as one body.
        const f = (m * alpha) / Math.max(l, DISTANCE_MIN2);
        fx += dx * f;
        fy += dy * f;
        continue;
      }
      // A leaf's mass is its one point's charge.
      if (isLeaf && p >= 0 && p !== i) {
        const px = (x[p] as number) - xi;
        const py = (y[p] as number) - yi;
        const f = (m * alpha) / Math.max(px * px + py * py, DISTANCE_MIN2);
        fx += px * f;
        fy += py * f;
      }
      if (!isLeaf) {
        for (let k = 0; k < 4; k++) {
          const c = this.child[kids + k] as number;
          if (c !== -1 && top < this.stack.length) this.stack[top++] = c;
        }
      }
    }
    vx[i] = (vx[i] as number) + fx;
    vy[i] = (vy[i] as number) + fy;
  }
}
