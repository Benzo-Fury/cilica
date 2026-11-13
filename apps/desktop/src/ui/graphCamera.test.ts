import { type Camera, MAX_SCALE, panBy, stepCamera, wheelZoomTarget } from "./graphCamera";

/** The world point at screen offset (px, py) from the view centre. */
function worldAt(c: Camera, px: number, py: number) {
  return { x: c.x + px / c.k, y: c.y + py / c.k };
}

/** Steps until arrival, returning every intermediate camera. */
function glide(camera: Camera, target: Parameters<typeof stepCamera>[1], ease = 0.18) {
  const frames: Camera[] = [];
  for (let i = 0; i < 500; i++) {
    const done = stepCamera(camera, target, ease);
    frames.push({ ...camera });
    if (done) break;
  }
  return frames;
}

describe("graph camera", () => {
  it("eases a wheel notch over several frames instead of jumping", () => {
    const camera = { x: 10, y: -5, k: 1 };
    const target = wheelZoomTarget(camera, null, 120, 80, -100);
    const frames = glide(camera, target);
    expect(frames.length).toBeGreaterThan(10);
    // The first frame covers only part of the change.
    const first = frames[0] as Camera;
    expect(first.k).toBeGreaterThan(1);
    expect(first.k).toBeLessThan(target.k * 0.5 + 0.5);
    // Scale grows monotonically and lands exactly on the target.
    for (let i = 1; i < frames.length; i++) {
      expect((frames[i] as Camera).k).toBeGreaterThanOrEqual((frames[i - 1] as Camera).k);
    }
    expect(camera).toEqual({ x: target.x, y: target.y, k: target.k });
  });

  it("keeps the point under the pointer still on every frame", () => {
    const camera = { x: 0, y: 0, k: 0.7 };
    const before = worldAt(camera, -200, 150);
    const target = wheelZoomTarget(camera, null, -200, 150, -240);
    for (const frame of glide(camera, target)) {
      const w = worldAt(frame, -200, 150);
      expect(w.x).toBeCloseTo(before.x, 6);
      expect(w.y).toBeCloseTo(before.y, 6);
    }
  });

  it("adds quick notches together rather than restarting from the camera", () => {
    const camera = { x: 0, y: 0, k: 1 };
    let target = wheelZoomTarget(camera, null, 0, 0, -50);
    stepCamera(camera, target, 0.18);
    target = wheelZoomTarget(camera, target, 0, 0, -50);
    expect(target.k).toBeCloseTo(Math.exp(1), 6);
  });

  it("respects the scale limits", () => {
    const camera = { x: 0, y: 0, k: 4 };
    const target = wheelZoomTarget(camera, null, 30, 30, -10_000);
    expect(target.k).toBe(MAX_SCALE);
  });

  it("jumps straight there with Reduce motion", () => {
    const camera = { x: 0, y: 0, k: 1 };
    const target = wheelZoomTarget(camera, null, 50, 0, -100);
    expect(stepCamera(camera, target, 1)).toBe(true);
    expect(camera.k).toBe(target.k);
  });

  it("carries a zoom glide along when panning mid-way", () => {
    const camera = { x: 0, y: 0, k: 1 };
    const target = wheelZoomTarget(camera, null, 100, 0, -100);
    stepCamera(camera, target, 0.18);
    panBy(camera, target, 40, -20);
    const expected = { x: target.x, y: target.y, k: target.k };
    glide(camera, target);
    expect(camera).toEqual(expected);
  });
});
