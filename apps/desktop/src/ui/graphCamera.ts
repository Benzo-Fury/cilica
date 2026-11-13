/**
 * Graph camera maths: where the camera is heading and how it gets there. Kept apart from the
 * canvas so the easing can be tested without a window.
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

export const MIN_SCALE = 0.08;
export const MAX_SCALE = 5;
/** Share of the remaining distance covered per frame. */
export const CAMERA_EASE = 0.18;

export function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

/**
 * The target after one wheel zoom step at screen offset (px, py) from the view centre. Steps
 * build on a glide already under way, so fast wheel notches and pinches add up instead of
 * restarting from wherever the camera happens to be.
 */
export function wheelZoomTarget(
  camera: Camera,
  target: CameraTarget | null,
  px: number,
  py: number,
  deltaY: number,
): CameraTarget {
  // The world point under the pointer right now: that is what must stay put.
  const wx = camera.x + px / camera.k;
  const wy = camera.y + py / camera.k;
  const from = target?.k ?? camera.k;
  const k = clamp(from * Math.exp(-deltaY * 0.01), MIN_SCALE, MAX_SCALE);
  return { x: wx - px / k, y: wy - py / k, k, anchor: { px, py, wx, wy } };
}

/** Moves the camera one frame toward the target. Returns true once it has arrived. */
export function stepCamera(camera: Camera, target: CameraTarget, ease: number): boolean {
  camera.k += (target.k - camera.k) * ease;
  if (target.anchor) {
    // Zoom about the anchor: derive the centre from the scale so the anchored point stays still.
    const a = target.anchor;
    camera.x = a.wx - a.px / camera.k;
    camera.y = a.wy - a.py / camera.k;
  } else {
    camera.x += (target.x - camera.x) * ease;
    camera.y += (target.y - camera.y) * ease;
  }
  const arrived =
    Math.abs(target.k - camera.k) < 0.002 &&
    Math.hypot(target.x - camera.x, target.y - camera.y) < 0.2;
  if (arrived) {
    camera.x = target.x;
    camera.y = target.y;
    camera.k = target.k;
  }
  return arrived;
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
