/**
 * The link graph, drawn on a canvas the way Obsidian draws its own: flat dots sized by how many
 * links they have, hairline edges, labels that fade in as you zoom, and on hover the node's
 * neighbours stay lit while everything else falls back.
 *
 * Cheap by design: the layout (graphSim) runs only while it settles or while you drag, and the
 * canvas redraws only when something changed. Idle, it uses no CPU. Labels are drawn only for
 * nodes on screen. Positions are remembered per `layoutKey`, so reopening a graph does not lay it
 * out again from scratch.
 */
import {
  type CSSProperties,
  type Ref,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
} from "react";
import { cn } from "./cn";
import {
  type CameraTarget,
  clamp,
  MAX_SCALE,
  MIN_SCALE,
  panBy,
  snapCamera,
  stepCamera,
  WheelClassifier,
  wheelZoomAmount,
  zeroVelocity,
  zoomTargetAt,
} from "./graphCamera";
import { APPEAR_SETTLE, appearOpacity, appearScale, GraphGrowth } from "./graphGrow";
import { HoverFade } from "./graphHover";
import { ForceSim } from "./graphSim";

/** Which colour token a node takes. */
export type GraphTone = "note" | "person" | "tag" | "missing" | "group";

export interface GraphViewNode {
  key: string;
  label: string;
  tone: GraphTone;
  /** For people and groups: the speaker colour, 1–8. */
  colorIndex?: number | null;
  /** Links in the full graph; sizes the dot. */
  degree: number;
}

export interface GraphViewEdge {
  source: number;
  target: number;
  /** Drawn dotted (a link read from a transcript, not written in a note). */
  dotted?: boolean;
}

export interface GraphHandle {
  zoomBy: (factor: number) => void;
  fit: () => void;
  centerOn: (key: string) => void;
  /**
   * Replays the graph growing: every node hides, then they appear step by step in `ranks` order
   * (one rank per node; equal ranks appear together) while the layout forms around them.
   */
  grow: (ranks: readonly number[]) => void;
  /** Ends a replay early, showing every node. */
  stopGrowing: () => void;
}

export interface GraphProps {
  nodes: readonly GraphViewNode[];
  edges: readonly GraphViewEdge[];
  /** Remembers positions across mounts (e.g. "global", "local"). */
  layoutKey: string;
  /** Drawn in the accent colour, labelled, and kept in the middle at first. */
  focusKey?: string | null;
  /** When set, other nodes fall back (search results). */
  highlight?: ReadonlySet<string> | null;
  /** "full" fills a view and pans with the scroll wheel; "inline" leaves scrolling to the page. */
  variant?: "full" | "inline";
  onOpen?: (key: string) => void;
  /** During a replay, the last step on screen; null when the replay ends or is stopped. */
  onGrowStep?: (step: number | null) => void;
  /** Spoken description of the graph. */
  label: string;
  className?: string;
  style?: CSSProperties;
  ref?: Ref<GraphHandle>;
}

interface Colors {
  note: string;
  missing: string;
  tag: string;
  focus: string;
  line: string;
  lineHot: string;
  text: string;
  halo: string;
  speakers: string[];
  font: string;
}

const FADE = 0.14;
/** How warm the layout is kept while a replay adds nodes, so it keeps forming around them. */
const GROW_HEAT = 0.3;
const positions = new Map<string, Map<string, { x: number; y: number }>>();

function readColors(el: Element): Colors {
  const css = getComputedStyle(el);
  const v = (name: string) => css.getPropertyValue(name).trim() || "#888";
  return {
    note: v("--ink-3"),
    missing: v("--ink-4"),
    tag: v("--ink-3"),
    focus: v("--accent"),
    line: v("--line-strong"),
    lineHot: v("--accent"),
    text: v("--ink-2"),
    halo: v("--graph-bg"),
    speakers: Array.from({ length: 8 }, (_, i) => v(`--speaker-${i + 1}`)),
    font: css.fontFamily,
  };
}

function radiusOf(degree: number): number {
  return Math.min(3.2 + Math.sqrt(degree) * 1.6, 15);
}

export function Graph({
  nodes,
  edges,
  layoutKey,
  focusKey = null,
  highlight = null,
  variant = "full",
  onOpen,
  onGrowStep,
  label,
  className,
  style,
  ref,
}: GraphProps) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // Everything the draw loop reads lives in one mutable object, so React renders never restart it.
  const live = useRef({
    sim: null as ForceSim | null,
    nodes,
    edges,
    neighbours: [] as number[][],
    focus: -1,
    highlight,
    camera: { x: 0, y: 0, k: 1 },
    target: null as CameraTarget | null,
    velocity: zeroVelocity(),
    wheel: new WheelClassifier(),
    autoFit: true,
    hover: -1,
    fade: new HoverFade(),
    lit: new Float32Array(0),
    width: 0,
    height: 0,
    colors: null as Colors | null,
    frame: 0,
    /** rAF time of the last frame while the loop runs, 0 when it is idle. */
    lastFrame: 0,
    drag: null as null | {
      node: number;
      startX: number;
      startY: number;
      lastX: number;
      lastY: number;
      moved: boolean;
    },
    reduced:
      typeof window !== "undefined" &&
      Boolean(window.matchMedia?.("(prefers-reduced-motion: reduce)").matches),
    /** The camera has been fitted to a real size at least once. */
    cameraReady: false,
    /** Seconds of frame time, for the grow replay's springs. */
    clock: 0,
    /** The replay in progress, if any. */
    growth: null as GraphGrowth | null,
    /** When each node appeared (on `clock`), NaN while hidden; null when every node is shown. */
    born: null as Float32Array | null,
    /** `clock` when the most recent node appeared. */
    lastBorn: Number.NEGATIVE_INFINITY,
    onOpen,
    onGrowStep,
  });
  live.current.onOpen = onOpen;
  live.current.onGrowStep = onGrowStep;

  // ── Drawing ──────────────────────────────────────────────────────────────
  const draw = useCallback(() => {
    const s = live.current;
    const canvas = canvasRef.current;
    const sim = s.sim;
    if (!canvas || !sim || !s.width) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    if (!s.colors) s.colors = readColors(canvas);
    const c = s.colors;
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, s.width, s.height);
    const { x: cx, y: cy, k } = s.camera;
    const ox = s.width / 2 - cx * k;
    const oy = s.height / 2 - cy * k;
    const sx = (i: number) => (sim.x[i] as number) * k + ox;
    const sy = (i: number) => (sim.y[i] as number) * k + oy;

    // Hover: every glow (the hovered node's, and those just left) lights its node and
    // neighbours; everything else falls back by the strongest glow.
    const fade = s.fade;
    const dim = fade.dim;
    if (s.lit.length !== s.nodes.length) s.lit = new Float32Array(s.nodes.length);
    const lit = fade.litness(s.neighbours, s.lit);
    const fallBack = (i: number) => 1 - (1 - FADE) * Math.max(0, dim - (lit[i] ?? 0));
    const searched = s.highlight;
    // Grow replay: hidden nodes are not drawn; new ones pop in on a spring.
    const born = s.born;
    const age = (i: number) => (born ? s.clock - (born[i] as number) : Number.POSITIVE_INFINITY);
    const hidden = (i: number) => born !== null && Number.isNaN(born[i]);
    const appearAlpha = (i: number) => (born ? appearOpacity(age(i)) : 1);
    const young = (i: number) => age(i) < APPEAR_SETTLE;
    const nodeAlpha = (i: number) => {
      let a = appearAlpha(i);
      if (dim > 0) a *= fallBack(i);
      if (searched && !searched.has(s.nodes[i]?.key ?? "")) a *= 0.22;
      return a;
    };

    // Edges: one path per style, faded ones first.
    const lineWidth = Math.max(0.5, Math.min(1, k * 0.9));
    ctx.lineWidth = lineWidth;
    for (const dotted of [false, true]) {
      ctx.setLineDash(dotted ? [2, 3] : []);
      ctx.strokeStyle = c.line;
      ctx.globalAlpha = (searched ? 0.35 : 1) * (1 - (1 - FADE) * dim);
      ctx.beginPath();
      for (const e of s.edges) {
        if (Boolean(e.dotted) !== dotted) continue;
        if (born && (hidden(e.source) || hidden(e.target) || young(e.source) || young(e.target)))
          continue;
        ctx.moveTo(sx(e.source), sy(e.source));
        ctx.lineTo(sx(e.target), sy(e.target));
      }
      ctx.stroke();
      // Edges of nodes still appearing fade in with them.
      if (born) {
        const base = ctx.globalAlpha;
        for (const e of s.edges) {
          if (Boolean(e.dotted) !== dotted) continue;
          if (hidden(e.source) || hidden(e.target)) continue;
          if (!young(e.source) && !young(e.target)) continue;
          ctx.globalAlpha = base * Math.min(appearAlpha(e.source), appearAlpha(e.target));
          ctx.beginPath();
          ctx.moveTo(sx(e.source), sy(e.source));
          ctx.lineTo(sx(e.target), sy(e.target));
          ctx.stroke();
        }
      }
      // Each glow's edges in the accent, as strong as the glow.
      for (const g of fade.glows) {
        if (g.amount <= 0) continue;
        ctx.strokeStyle = c.lineHot;
        ctx.globalAlpha = g.amount;
        ctx.lineWidth = lineWidth * 1.4;
        ctx.beginPath();
        for (const e of s.edges) {
          if (Boolean(e.dotted) !== dotted || (e.source !== g.node && e.target !== g.node))
            continue;
          if (hidden(e.source) || hidden(e.target)) continue;
          ctx.moveTo(sx(e.source), sy(e.source));
          ctx.lineTo(sx(e.target), sy(e.target));
        }
        ctx.stroke();
        ctx.lineWidth = lineWidth;
      }
    }
    ctx.setLineDash([]);

    // Nodes.
    const colorOf = (n: GraphViewNode, i: number) => {
      if (i === s.focus) return c.focus;
      switch (n.tone) {
        case "person":
        case "group":
          return c.speakers[((n.colorIndex ?? 1) - 1 + 8) % 8] ?? c.note;
        case "tag":
          return c.tag;
        case "missing":
          return c.missing;
        default:
          return c.note;
      }
    };
    const pad = 40;
    const visible = (px: number, py: number) =>
      px > -pad && py > -pad && px < s.width + pad && py < s.height + pad;
    for (let i = 0; i < s.nodes.length; i++) {
      const n = s.nodes[i] as GraphViewNode;
      const px = sx(i);
      const py = sy(i);
      if (!visible(px, py) || hidden(i)) continue;
      const grow = s.reduced ? 0 : 0.15 * fade.amountOf(i);
      const pop = s.reduced || !born ? 1 : appearScale(age(i));
      const r = Math.max(radiusOf(n.degree) * k, 1.6) * (1 + grow) * pop;
      if (r <= 0) continue;
      ctx.globalAlpha = nodeAlpha(i);
      ctx.beginPath();
      if (n.tone === "tag" && i !== s.focus) {
        // Tags are small diamonds, so they read apart from notes and people at any colour.
        const d = r * 1.15;
        ctx.moveTo(px, py - d);
        ctx.lineTo(px + d, py);
        ctx.lineTo(px, py + d);
        ctx.lineTo(px - d, py);
        ctx.closePath();
      } else {
        ctx.arc(px, py, r, 0, Math.PI * 2);
      }
      if (n.tone === "missing" && i !== s.focus) {
        ctx.fillStyle = c.halo;
        ctx.fill();
        ctx.lineWidth = Math.max(1, r * 0.28);
        ctx.strokeStyle = c.missing;
        ctx.stroke();
      } else {
        ctx.fillStyle = colorOf(n, i);
        ctx.fill();
      }
      // People wear a thin ring, so they read apart from notes in any colour.
      if (n.tone === "person" && i !== s.focus) {
        ctx.globalAlpha = nodeAlpha(i) * 0.55;
        ctx.lineWidth = 1;
        ctx.strokeStyle = colorOf(n, i);
        ctx.beginPath();
        ctx.arc(px, py, r + Math.max(2, r * 0.45), 0, Math.PI * 2);
        ctx.stroke();
      }
      if (i === s.focus) {
        ctx.globalAlpha = nodeAlpha(i) * 0.25;
        ctx.lineWidth = 3;
        ctx.strokeStyle = c.focus;
        ctx.beginPath();
        ctx.arc(px, py, r + 3.5, 0, Math.PI * 2);
        ctx.stroke();
      }
    }

    // Labels: in screen space, faded in by zoom; bigger nodes earlier. Hovered, focused and
    // searched nodes always.
    const fontSize = Math.round(Math.min(13, Math.max(11, 11 + (k - 1) * 1.5)));
    ctx.font = `450 ${fontSize}px ${c.font}`;
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    ctx.lineJoin = "round";
    ctx.lineWidth = 3;
    ctx.strokeStyle = c.halo;
    for (let i = 0; i < s.nodes.length; i++) {
      const n = s.nodes[i] as GraphViewNode;
      if (!n.label) continue;
      const px = sx(i);
      const py = sy(i);
      if (!visible(px, py) || hidden(i)) continue;
      // While growing, the newest notes are named as they arrive.
      const newest = s.growth !== null && s.growth.ranks[i] === s.growth.shown - 1;
      const always =
        newest ||
        i === s.focus ||
        (searched?.has(n.key) ?? false) ||
        (variant === "inline" && s.nodes.length <= 24);
      // Fade in between these zoom levels, earlier for well-linked nodes.
      // A short ramp: a label is readable or not drawn, rarely a faint smudge in between.
      const start = 0.72 - Math.min(0.32, Math.sqrt(n.degree) * 0.08);
      const zoomAlpha = Math.max(0, Math.min(1, (k - start) / 0.18));
      // Lit nodes' labels fade in with their glow.
      let a = Math.max(always ? 1 : zoomAlpha, lit[i] ?? 0);
      if (dim > 0) a *= fallBack(i);
      if (searched && !searched.has(n.key)) a *= 0.22;
      a *= appearAlpha(i);
      if (a < 0.02) continue;
      const r = Math.max(radiusOf(n.degree) * k, 1.6);
      const text = n.label.length > 42 ? `${n.label.slice(0, 40)}…` : n.label;
      ctx.globalAlpha = a;
      ctx.strokeText(text, px, py + r + 4);
      ctx.fillStyle = c.text;
      ctx.fillText(text, px, py + r + 4);
    }
    ctx.globalAlpha = 1;
  }, [variant]);

  // ── The loop: runs only while something moves ─────────────────────────────
  const step = useCallback(
    (now: number) => {
      const s = live.current;
      s.frame = 0;
      const sim = s.sim;
      if (!sim) return;
      // Seconds since the last frame, so motion runs at the same speed at any refresh rate. A loop
      // that was idle starts with one nominal frame; a long stall is capped so nothing leaps.
      const dt = s.lastFrame ? clamp((now - s.lastFrame) / 1000, 0, 0.05) : 1 / 60;
      s.lastFrame = now;
      s.clock += dt;
      let busy = false;
      if (s.growth) {
        const [from, to] = s.growth.advance(dt);
        if (to > from) revealSteps(s, from, to);
        if (s.growth?.done) endGrowth(s);
        busy = true;
      }
      if (s.born && s.clock - s.lastBorn < APPEAR_SETTLE) busy = true;
      if (!sim.settled) {
        sim.tick();
        busy = true;
      }
      // Hover highlight crossfades from node to node (an opacity fade, kept under Reduce motion).
      if (s.fade.step(s.hover, dt)) busy = true;
      // While the first layout settles, the camera keeps it all in view.
      if (s.autoFit && !sim.settled) s.target = fitCamera(s, sim);
      if (s.target) {
        if (s.reduced) {
          snapCamera(s.camera, s.velocity, s.target);
          s.target = null;
        } else if (stepCamera(s.camera, s.velocity, s.target, dt)) {
          s.target = null;
        } else {
          busy = true;
        }
      }
      draw();
      if (busy) s.frame = requestAnimationFrame(step);
      else s.lastFrame = 0;
    },
    [draw],
  );

  const kick = useCallback(() => {
    const s = live.current;
    if (!s.frame) s.frame = requestAnimationFrame(step);
  }, [step]);

  // ── Data in ───────────────────────────────────────────────────────────────
  useEffect(() => {
    const s = live.current;
    const remembered = positions.get(layoutKey) ?? new Map<string, { x: number; y: number }>();
    positions.set(layoutKey, remembered);
    // Keep where nodes were (this graph, or the one before a refetch).
    const prev = s.sim;
    const prevIndex = new Map(s.nodes.map((n, i) => [n.key, i]));
    const neighbours: number[][] = nodes.map(() => []);
    for (const e of edges) {
      neighbours[e.source]?.push(e.target);
      neighbours[e.target]?.push(e.source);
    }
    let known = 0;
    const place = (i: number) => {
      const key = nodes[i]?.key ?? "";
      const pi = prev ? prevIndex.get(key) : undefined;
      if (prev && pi !== undefined && pi < prev.n) {
        known++;
        return { x: prev.x[pi] as number, y: prev.y[pi] as number };
      }
      const r = remembered.get(key);
      if (r) known++;
      return r ?? null;
    };
    const initial = nodes.map((_, i) => place(i));
    // A new node starts next to a neighbour that already has a place.
    for (let i = 0; i < nodes.length; i++) {
      if (initial[i]) continue;
      const near = (neighbours[i] ?? []).map((j) => initial[j]).find(Boolean);
      if (near)
        initial[i] = {
          x: near.x + (Math.random() - 0.5) * 30,
          y: near.y + (Math.random() - 0.5) * 30,
        };
    }
    const inline = variant === "inline";
    const sim = new ForceSim(
      nodes.length,
      edges,
      {
        linkDistance: inline ? 84 : 66,
        charge: inline ? -360 : -280,
        gravity: inline ? 0.1 : 0.07,
      },
      (i) => initial[i] ?? null,
    );
    const mostlyKnown = nodes.length > 0 && known / nodes.length > 0.9;
    sim.alpha = mostlyKnown ? 0.12 : 1;
    s.sim = sim;
    s.nodes = nodes;
    s.edges = edges;
    s.neighbours = neighbours;
    s.focus = focusKey ? nodes.findIndex((n) => n.key === focusKey) : -1;
    s.hover = -1;
    s.fade.reset();
    if (s.growth) s.onGrowStep?.(null);
    s.growth = null;
    s.born = null;
    if (!prev) s.autoFit = !mostlyKnown;
    if (!prev && mostlyKnown && s.width) {
      s.camera = fitCamera(s, sim);
      s.cameraReady = true;
    }
    // Reduce motion: settle now and show the result still.
    if (s.reduced) {
      const until = performance.now() + 250;
      while (!sim.settled && performance.now() < until) sim.tick();
      sim.alpha = 0;
      if (s.width) {
        s.camera = fitCamera(s, sim);
        s.cameraReady = true;
      }
      s.target = null;
    }
    kick();
    return () => {
      // Remember where everything ended up.
      for (let i = 0; i < sim.n; i++) {
        const key = nodes[i]?.key;
        if (key) remembered.set(key, { x: sim.x[i] as number, y: sim.y[i] as number });
      }
    };
  }, [nodes, edges, layoutKey, focusKey, variant, kick]);

  useEffect(() => {
    live.current.highlight = highlight;
    kick();
  }, [highlight, kick]);

  // ── Size, theme, motion preference ────────────────────────────────────────
  useEffect(() => {
    const wrap = wrapRef.current;
    const canvas = canvasRef.current;
    if (!wrap || !canvas) return;
    const s = live.current;
    const resize = () => {
      const rect = wrap.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      s.width = rect.width;
      s.height = rect.height;
      canvas.width = Math.max(1, Math.round(rect.width * dpr));
      canvas.height = Math.max(1, Math.round(rect.height * dpr));
      canvas.style.width = `${rect.width}px`;
      canvas.style.height = `${rect.height}px`;
      if ((s.autoFit || !s.cameraReady) && s.sim && s.width) {
        s.camera = fitCamera(s, s.sim);
        s.cameraReady = true;
      }
      draw();
    };
    const observer = new ResizeObserver(resize);
    observer.observe(wrap);
    resize();
    const recolor = () => {
      s.colors = null;
      draw();
    };
    const theme = new MutationObserver(recolor);
    theme.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme", "class", "style"],
    });
    const scheme = window.matchMedia("(prefers-color-scheme: dark)");
    scheme.addEventListener("change", recolor);
    const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const setReduced = () => {
      s.reduced = motion.matches;
    };
    setReduced();
    motion.addEventListener("change", setReduced);
    return () => {
      observer.disconnect();
      theme.disconnect();
      scheme.removeEventListener("change", recolor);
      motion.removeEventListener("change", setReduced);
      if (s.frame) cancelAnimationFrame(s.frame);
      s.frame = 0;
    };
  }, [draw]);

  // ── Camera controls ───────────────────────────────────────────────────────
  useImperativeHandle(
    ref,
    () => ({
      zoomBy: (factor: number) => {
        const s = live.current;
        s.autoFit = false;
        const { x, y, k } = s.target ?? s.camera;
        s.target = { x, y, k: clamp(k * factor, MIN_SCALE, MAX_SCALE) };
        kick();
      },
      fit: () => {
        const s = live.current;
        if (!s.sim) return;
        s.autoFit = false;
        s.target = fitCamera(s, s.sim);
        kick();
      },
      centerOn: (key: string) => {
        const s = live.current;
        const i = s.nodes.findIndex((n) => n.key === key);
        if (i < 0 || !s.sim) return;
        s.autoFit = false;
        s.target = {
          x: s.sim.x[i] as number,
          y: s.sim.y[i] as number,
          k: Math.max(s.camera.k, 1.4),
        };
        kick();
      },
      grow: (ranks: readonly number[]) => {
        const s = live.current;
        const sim = s.sim;
        if (!sim || sim.n === 0) return;
        s.growth = new GraphGrowth(Array.from({ length: sim.n }, (_, i) => ranks[i] ?? 0));
        s.born = new Float32Array(sim.n).fill(Number.NaN);
        s.hover = -1;
        s.fade.reset();
        if (s.reduced) {
          // Reduce motion: the layout stays as it is and nodes fade in where they already are.
          s.target = null;
        } else {
          // The layout forms again around the nodes as they arrive, with the camera following.
          sim.active.fill(0);
          sim.alphaTarget = GROW_HEAT;
          sim.reheat(0.6);
          s.autoFit = true;
        }
        kick();
      },
      stopGrowing: () => {
        const s = live.current;
        if (!s.growth) return;
        const [from, to] = s.growth.finish();
        revealSteps(s, from, to);
        endGrowth(s);
        kick();
      },
    }),
    [kick],
  );

  // ── Pointer ───────────────────────────────────────────────────────────────
  const toWorld = (clientX: number, clientY: number) => {
    const s = live.current;
    const rect = canvasRef.current?.getBoundingClientRect();
    const px = clientX - (rect?.left ?? 0);
    const py = clientY - (rect?.top ?? 0);
    return {
      px,
      py,
      x: (px - s.width / 2) / s.camera.k + s.camera.x,
      y: (py - s.height / 2) / s.camera.k + s.camera.y,
    };
  };

  const nodeAt = (clientX: number, clientY: number): number => {
    const s = live.current;
    const sim = s.sim;
    if (!sim) return -1;
    const { x, y } = toWorld(clientX, clientY);
    let best = -1;
    let bestD = Number.POSITIVE_INFINITY;
    const slop = 5 / s.camera.k;
    for (let i = 0; i < sim.n; i++) {
      if (s.born && Number.isNaN(s.born[i])) continue;
      const dx = (sim.x[i] as number) - x;
      const dy = (sim.y[i] as number) - y;
      const d = dx * dx + dy * dy;
      const r = Math.max(radiusOf(s.nodes[i]?.degree ?? 0), 1.6 / s.camera.k) + slop;
      if (d < r * r && d < bestD) {
        best = i;
        bestD = d;
      }
    }
    return best;
  };

  const setHover = (i: number) => {
    const s = live.current;
    if (s.hover === i) return;
    s.hover = i;
    if (canvasRef.current)
      canvasRef.current.style.cursor = i >= 0 ? "pointer" : s.drag ? "grabbing" : "";
    kick();
  };

  const onPointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return;
    const s = live.current;
    e.currentTarget.setPointerCapture(e.pointerId);
    s.drag = {
      node: nodeAt(e.clientX, e.clientY),
      startX: e.clientX,
      startY: e.clientY,
      lastX: e.clientX,
      lastY: e.clientY,
      moved: false,
    };
  };

  const onPointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const s = live.current;
    const d = s.drag;
    if (!d) {
      setHover(nodeAt(e.clientX, e.clientY));
      return;
    }
    if (!d.moved && Math.hypot(e.clientX - d.startX, e.clientY - d.startY) < 4) return;
    d.moved = true;
    s.autoFit = false;
    const sim = s.sim;
    if (d.node >= 0 && sim) {
      // Move the node; its neighbours follow through the springs.
      const { x, y } = toWorld(e.clientX, e.clientY);
      sim.pinned[d.node] = 1;
      sim.x[d.node] = x;
      sim.y[d.node] = y;
      sim.alphaTarget = 0.25;
      sim.reheat(0.25);
      setHover(d.node);
    } else {
      const k = s.camera.k;
      s.camera.x -= (e.clientX - d.lastX) / k;
      s.camera.y -= (e.clientY - d.lastY) / k;
      s.target = null;
      s.velocity = zeroVelocity();
      if (canvasRef.current) canvasRef.current.style.cursor = "grabbing";
    }
    d.lastX = e.clientX;
    d.lastY = e.clientY;
    kick();
  };

  const endDrag = (e: React.PointerEvent<HTMLCanvasElement>, cancelled: boolean) => {
    const s = live.current;
    const d = s.drag;
    s.drag = null;
    if (!d) return;
    if (d.node >= 0 && s.sim) {
      s.sim.pinned[d.node] = 0;
      s.sim.alphaTarget = s.growth && !s.reduced ? GROW_HEAT : 0;
    }
    if (canvasRef.current) canvasRef.current.style.cursor = s.hover >= 0 ? "pointer" : "";
    if (!cancelled && !d.moved && d.node >= 0) {
      const key = s.nodes[d.node]?.key;
      if (key) s.onOpen?.(key);
    }
    if (e.currentTarget.hasPointerCapture(e.pointerId))
      e.currentTarget.releasePointerCapture(e.pointerId);
    kick();
  };

  // Wheel: a mouse wheel, a pinch, or ⌘/Ctrl + wheel zooms at the pointer; two-finger scroll
  // pans the full graph. An inline graph zooms only with a pinch or ⌘/Ctrl, and otherwise leaves
  // scrolling to the page. Native listener: React's is passive.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const onWheel = (e: WheelEvent) => {
      const s = live.current;
      const input = {
        deltaMode: e.deltaMode,
        deltaX: e.deltaX,
        deltaY: e.deltaY,
        wheelDeltaY: (e as WheelEvent & { wheelDeltaY?: number }).wheelDeltaY,
      };
      const device = s.wheel.classify(input, e.timeStamp);
      const mouseZoom = device === "mouse" && variant === "full" && !e.shiftKey;
      const zoom = e.ctrlKey || e.metaKey || mouseZoom;
      if (!zoom && variant === "inline") return;
      e.preventDefault();
      s.autoFit = false;
      if (zoom) {
        // Spring toward the new scale with the point under the cursor held still. A mouse notch
        // is one fixed step, so a coarse wheel glides like a trackpad pinch instead of jumping.
        const rect = canvas.getBoundingClientRect();
        const px = e.clientX - rect.left - s.width / 2;
        const py = e.clientY - rect.top - s.height / 2;
        const amount = wheelZoomAmount(input, device);
        s.target = zoomTargetAt(s.camera, s.target?.anchor ? s.target : null, px, py, amount);
      } else {
        // Pans stay immediate (they track the fingers) and carry any zoom glide along.
        if (!s.target?.anchor) {
          s.target = null;
          s.velocity = zeroVelocity();
        }
        panBy(s.camera, s.target, e.deltaX / s.camera.k, e.deltaY / s.camera.k);
      }
      kick();
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    return () => canvas.removeEventListener("wheel", onWheel);
  }, [variant, kick]);

  const onKeyDown = (e: React.KeyboardEvent<HTMLCanvasElement>) => {
    const s = live.current;
    const pan = 60 / s.camera.k;
    const moves: Record<string, [number, number]> = {
      ArrowLeft: [-pan, 0],
      ArrowRight: [pan, 0],
      ArrowUp: [0, -pan],
      ArrowDown: [0, pan],
    };
    const move = moves[e.key];
    if (move) {
      s.autoFit = false;
      s.target = { x: s.camera.x + move[0], y: s.camera.y + move[1], k: s.camera.k };
    } else if (e.key === "+" || e.key === "=") {
      s.target = { ...s.camera, k: clamp(s.camera.k * 1.25, MIN_SCALE, MAX_SCALE) };
    } else if (e.key === "-") {
      s.target = { ...s.camera, k: clamp(s.camera.k / 1.25, MIN_SCALE, MAX_SCALE) };
    } else if (e.key === "0" && s.sim) {
      s.target = fitCamera(s, s.sim);
    } else {
      return;
    }
    e.preventDefault();
    kick();
  };

  return (
    <div
      ref={wrapRef}
      className={cn("nt-graph", variant === "inline" && "nt-graph-inline", className)}
      style={style}
    >
      <canvas
        ref={canvasRef}
        className="nt-graph-canvas"
        role="img"
        aria-label={label}
        tabIndex={0}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={(e) => endDrag(e, false)}
        onPointerCancel={(e) => endDrag(e, true)}
        onPointerLeave={() => !live.current.drag && setHover(-1)}
        onKeyDown={onKeyDown}
      />
    </div>
  );
}

/** The camera that shows every node with some room around it. */
function fitCamera(
  s: {
    width: number;
    height: number;
    nodes: readonly GraphViewNode[];
    born?: Float32Array | null;
  },
  sim: ForceSim,
): { x: number; y: number; k: number } {
  if (sim.n === 0 || !s.width) return { x: 0, y: 0, k: 1 };
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  let shown = 0;
  for (let i = 0; i < sim.n; i++) {
    if (s.born && Number.isNaN(s.born[i])) continue;
    shown++;
    const r = radiusOf(s.nodes[i]?.degree ?? 0);
    minX = Math.min(minX, (sim.x[i] as number) - r);
    maxX = Math.max(maxX, (sim.x[i] as number) + r);
    minY = Math.min(minY, (sim.y[i] as number) - r);
    maxY = Math.max(maxY, (sim.y[i] as number) + r + 16);
  }
  // Nothing on screen yet (a replay's first beat): look at the middle.
  if (shown === 0) return { x: 0, y: 0, k: 1.6 };
  // Room for labels at the edges (and the key in the corner of the full graph).
  const pad = clamp(Math.min(s.width, s.height) * 0.09, 24, 64);
  const k = clamp(
    Math.min(
      (s.width - pad * 2) / Math.max(maxX - minX, 1),
      (s.height - pad * 2) / Math.max(maxY - minY, 1),
    ),
    MIN_SCALE,
    1.6,
  );
  return { x: (minX + maxX) / 2, y: (minY + maxY) / 2, k };
}

type Live = {
  sim: ForceSim | null;
  neighbours: number[][];
  growth: GraphGrowth | null;
  born: Float32Array | null;
  clock: number;
  lastBorn: number;
  reduced: boolean;
  onGrowStep?: (step: number | null) => void;
};

/**
 * Brings the nodes of steps `[from, to)` of the replay into view. Each one starts beside a
 * neighbour already on screen (or near the middle of what is), so the layout grows outward.
 */
function revealSteps(s: Live, from: number, to: number) {
  const { sim, growth, born } = s;
  if (!sim || !growth || !born || to <= from) return;
  const spread = sim.options.linkDistance;
  // Middle of what is on screen, for nodes with no neighbour there yet.
  let cx = 0;
  let cy = 0;
  let count = 0;
  for (let i = 0; i < sim.n; i++) {
    if (Number.isNaN(born[i])) continue;
    cx += sim.x[i] as number;
    cy += sim.y[i] as number;
    count++;
  }
  if (count) {
    cx /= count;
    cy /= count;
  }
  for (let i = 0; i < sim.n; i++) {
    const r = growth.ranks[i] as number;
    if (r < from || r >= to || !Number.isNaN(born[i])) continue;
    born[i] = s.clock;
    if (s.reduced) continue;
    const near = (s.neighbours[i] ?? []).find((j) => !Number.isNaN(born[j]) && j !== i);
    const angle = Math.random() * Math.PI * 2;
    if (near !== undefined) {
      sim.x[i] = (sim.x[near] as number) + Math.cos(angle) * spread * 0.5;
      sim.y[i] = (sim.y[near] as number) + Math.sin(angle) * spread * 0.5;
    } else {
      const ring = count ? spread * (0.8 + Math.random() * 0.6) : spread * 0.1;
      sim.x[i] = cx + Math.cos(angle) * ring;
      sim.y[i] = cy + Math.sin(angle) * ring;
    }
    sim.vx[i] = 0;
    sim.vy[i] = 0;
    sim.active[i] = 1;
  }
  s.lastBorn = s.clock;
  s.onGrowStep?.(to - 1);
}

/** The replay is over: every node takes part in the layout again, and it cools down. */
function endGrowth(s: Live) {
  const { sim } = s;
  s.growth = null;
  if (sim) {
    sim.active.fill(1);
    sim.alphaTarget = 0;
  }
  s.onGrowStep?.(null);
}
