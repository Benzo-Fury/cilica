/**
 * Graph camera maths: where the camera is heading, how it gets there, and how much a wheel event
 * should zoom. Kept apart from the canvas so the motion can be tested without a window.
 */

export interface Camera {
  /** World point at the centre of the view. */
  x: number;
  y: number;
  /** Scale: screen pixels per world unit. */
  k: number;
}

/** A camera target. With `anchor`, the world point (wx, wy) stays under the screen offset
 * (px, py) from the view centre for the whole glide, so zooming at the pointer never drifts. */
export interface CameraTarget extends Camera {
  anchor?: { px: number; py: number; wx: number; wy: number };
}

/** How fast the camera is moving: world units per second, and log-scale per second. */
export interface CameraVelocity {
  x: number;
  y: number;
  lk: number;
}

export const MIN_SCALE = 0.08;
export const MAX_SCALE = 5;

/**
 * The camera's spring: critically damped (no bounce, so it never overshoots the scale limits),
 * with the same visual duration as the `smooth` preset in motion.md. A spring keeps its velocity
 * when retargeted, so a run of wheel notches reads as one continuous glide.
 */
export const CAMERA_OMEGA = (2 * Math.PI) / 0.3;

/** Zoom per mouse-wheel notch. */
export const NOTCH_ZOOM = 1.2;
/** Log-scale zoom per pixel of trackpad pinch (or pixel-mode wheel) delta. */
const PINCH_RATE = 0.01;
/** A pause longer than this between wheel events starts a new gesture. */
export const WHEEL_GESTURE_GAP_MS = 250;

export function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

export function zeroVelocity(): CameraVelocity {
  return { x: 0, y: 0, lk: 0 };
}

// ── Wheel input ─────────────────────────────────────────────────────────────

/** The parts of a WheelEvent the camera reads. `wheelDeltaY` is the legacy, non-standard field
 * that browsers still fill: ±120 per notch for a mouse wheel. */
export interface WheelInput {
  deltaMode: number;
  deltaX: number;
  deltaY: number;
  wheelDeltaY?: number;
}

export type WheelDevice = "mouse" | "trackpad";

/** What one event on its own looks like it came from. */
function guessDevice(e: WheelInput): WheelDevice {
  if (e.deltaMode !== 0) return "mouse";
  if (e.deltaX !== 0) return "trackpad";
  const legacy = e.wheelDeltaY ?? 0;
  return legacy !== 0 && legacy % 120 === 0 ? "mouse" : "trackpad";
}

/**
 * Tells a mouse wheel from a trackpad. The first event of a gesture decides, and the rest of the
 * gesture keeps that answer, so a trackpad delta that happens to look like a notch mid-scroll
 * doesn't suddenly zoom.
 */
export class WheelClassifier {
  private device: WheelDevice = "trackpad";
  private last = Number.NEGATIVE_INFINITY;

  classify(e: WheelInput, now: number): WheelDevice {
    if (now - this.last > WHEEL_GESTURE_GAP_MS) this.device = guessDevice(e);
    this.last = now;
    return this.device;
  }
}

/** How much one wheel event zooms, as a change in log-scale (positive zooms in). */
export function wheelZoomAmount(e: WheelInput, device: WheelDevice): number {
  if (device === "mouse") {
    // Count notches, whatever the OS's lines-per-notch setting, and zoom a fixed step for each.
    const legacy = e.wheelDeltaY ?? 0;
    let notches: number;
    if (legacy !== 0 && legacy % 120 === 0) notches = -legacy / 120;
    else if (e.deltaMode === 1) notches = e.deltaY / 3;
    else if (e.deltaMode === 2) notches = e.deltaY;
    else notches = Math.sign(e.deltaY);
    return -clamp(notches, -4, 4) * Math.log(NOTCH_ZOOM);
  }
  const dy = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
  return clamp(-dy * PINCH_RATE, -0.5, 0.5);
}

// ── Targets and motion ──────────────────────────────────────────────────────

/**
 * The target after a zoom of `amount` (log-scale) at screen offset (px, py) from the view centre.
 * Steps build on a glide already under way, so fast notches and pinches add up instead of
 * restarting from wherever the camera happens to be.
 */
export function zoomTargetAt(
  camera: Camera,
  target: CameraTarget | null,
  px: number,
  py: number,
  amount: number,
): CameraTarget {
  // The world point under the pointer right now: that is what must stay put.
  const wx = camera.x + px / camera.k;
  const wy = camera.y + py / camera.k;
  const from = target?.k ?? camera.k;
  const k = clamp(from * Math.exp(amount), MIN_SCALE, MAX_SCALE);
  return { x: wx - px / k, y: wy - py / k, k, anchor: { px, py, wx, wy } };
}

/** One critically damped spring step for displacement `d` (current − target) over `dt` seconds.
 * Exact for any dt, so a dropped frame never overshoots. */
function springStep(d: number, v: number, dt: number, omega: number): [number, number] {
  const c = v + omega * d;
  const decay = Math.exp(-omega * dt);
  return [(d + c * dt) * decay, (c - omega * (d + c * dt)) * decay];
}

/**
 * Moves the camera `dt` seconds toward the target. Scale springs in log space, so zooming in and
 * out feel the same at every zoom level. Returns true once it has arrived.
 */
export function stepCamera(
  camera: Camera,
  velocity: CameraVelocity,
  target: CameraTarget,
  dt: number,
  omega = CAMERA_OMEGA,
): boolean {
  const tlk = Math.log(target.k);
  const [dlk, vlk] = springStep(Math.log(camera.k) - tlk, velocity.lk, dt, omega);
  camera.k = Math.exp(tlk + dlk);
  velocity.lk = vlk;
  if (target.anchor) {
    // Zoom about the anchor: derive the centre from the scale so the anchored point stays still.
    const a = target.anchor;
    camera.x = a.wx - a.px / camera.k;
    camera.y = a.wy - a.py / camera.k;
    velocity.x = 0;
    velocity.y = 0;
  } else {
    const [dx, vx] = springStep(camera.x - target.x, velocity.x, dt, omega);
    const [dy, vy] = springStep(camera.y - target.y, velocity.y, dt, omega);
    camera.x = target.x + dx;
    camera.y = target.y + dy;
    velocity.x = vx;
    velocity.y = vy;
  }
  // Arrived once the rest is under a third of a screen pixel and barely moving.
  const k = camera.k;
  const arrived =
    Math.abs(dlk) < 5e-4 &&
    Math.abs(vlk) < 5e-3 &&
    Math.hypot(target.x - camera.x, target.y - camera.y) * k < 0.3 &&
    Math.hypot(velocity.x, velocity.y) * k < 3;
  if (arrived) snapCamera(camera, velocity, target);
  return arrived;
}

/** Jumps straight to the target (Reduce motion, or arrival). */
export function snapCamera(camera: Camera, velocity: CameraVelocity, target: CameraTarget): void {
  camera.x = target.x;
  camera.y = target.y;
  camera.k = target.k;
  velocity.x = 0;
  velocity.y = 0;
  velocity.lk = 0;
}

/** Shifts a camera and its target by a world-space pan, keeping any zoom glide going. */
export function panBy(camera: Camera, target: CameraTarget | null, dx: number, dy: number): void {
  camera.x += dx;
  camera.y += dy;
  if (!target) return;
  target.x += dx;
  target.y += dy;
  if (target.anchor) {
    target.anchor.wx += dx;
    target.anchor.wy += dy;
  }
}
