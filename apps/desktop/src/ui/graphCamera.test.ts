import {
  type Camera,
  type CameraTarget,
  MAX_SCALE,
  NOTCH_ZOOM,
  panBy,
  stepCamera,
  WHEEL_GESTURE_GAP_MS,
  WheelClassifier,
  wheelZoomAmount,
  zeroVelocity,
  zoomTargetAt,
} from "./graphCamera";

/** The world point at screen offset (px, py) from the view centre. */
function worldAt(c: Camera, px: number, py: number) {
  return { x: c.x + px / c.k, y: c.y + py / c.k };
}

/** Steps at `hz` until arrival, returning every intermediate camera. */
function glide(camera: Camera, target: CameraTarget, hz = 60, velocity = zeroVelocity()) {
  const frames: Camera[] = [];
  for (let i = 0; i < 2000; i++) {
    const done = stepCamera(camera, velocity, target, 1 / hz);
    frames.push({ ...camera });
    if (done) break;
  }
  return frames;
}

/** Largest single-frame change in log-scale, as a share of the whole zoom. */
function biggestFrameShare(start: Camera, frames: Camera[], target: CameraTarget) {
  const total = Math.abs(Math.log(target.k / start.k));
  let prev = start.k;
  let worst = 0;
  for (const f of frames) {
    worst = Math.max(worst, Math.abs(Math.log(f.k / prev)) / total);
    prev = f.k;
  }
  return worst;
}

const notch = (down: boolean) => ({
  deltaMode: 0,
  deltaX: 0,
  deltaY: down ? 100 : -100,
  wheelDeltaY: down ? -120 : 120,
});

describe("wheel input", () => {
  it("tells a mouse notch from a trackpad scroll", () => {
    expect(new WheelClassifier().classify(notch(true), 0)).toBe("mouse");
    const pad = { deltaMode: 0, deltaX: 0, deltaY: 3.5, wheelDeltaY: -10 };
    expect(new WheelClassifier().classify(pad, 0)).toBe("trackpad");
    const diagonal = { deltaMode: 0, deltaX: 2, deltaY: 40, wheelDeltaY: -120 };
    expect(new WheelClassifier().classify(diagonal, 0)).toBe("trackpad");
    const lines = { deltaMode: 1, deltaX: 0, deltaY: 3 };
    expect(new WheelClassifier().classify(lines, 0)).toBe("mouse");
  });

  it("keeps a gesture's device until the wheel pauses", () => {
    const c = new WheelClassifier();
    expect(c.classify({ deltaMode: 0, deltaX: 0, deltaY: 4, wheelDeltaY: -12 }, 0)).toBe(
      "trackpad",
    );
    // A trackpad delta that happens to look like a notch mid-gesture stays a trackpad.
    expect(c.classify({ deltaMode: 0, deltaX: 0, deltaY: 40, wheelDeltaY: -120 }, 30)).toBe(
      "trackpad",
    );
    expect(c.classify(notch(true), 30 + WHEEL_GESTURE_GAP_MS + 1)).toBe("mouse");
  });

  it("zooms one fixed step per mouse notch, whatever the delta size", () => {
    expect(wheelZoomAmount(notch(false), "mouse")).toBeCloseTo(Math.log(NOTCH_ZOOM), 9);
    expect(wheelZoomAmount(notch(true), "mouse")).toBeCloseTo(-Math.log(NOTCH_ZOOM), 9);
    const bigLines = { deltaMode: 0, deltaX: 0, deltaY: 300, wheelDeltaY: -120 };
    expect(wheelZoomAmount(bigLines, "mouse")).toBeCloseTo(-Math.log(NOTCH_ZOOM), 9);
  });

  it("follows a pinch's deltas in proportion, within a limit", () => {
    const small = wheelZoomAmount({ deltaMode: 0, deltaX: 0, deltaY: -2 }, "trackpad");
    expect(small).toBeCloseTo(0.02, 9);
    const huge = wheelZoomAmount({ deltaMode: 0, deltaX: 0, deltaY: -1000 }, "trackpad");
    expect(huge).toBe(0.5);
  });
});

describe("graph camera", () => {
  it("glides a mouse notch over many frames with no frame taking a big share", () => {
    const camera = { x: 10, y: -5, k: 1 };
    const start = { ...camera };
    const target = zoomTargetAt(camera, null, 120, 80, wheelZoomAmount(notch(false), "mouse"));
    const frames = glide(camera, target);
    expect(frames.length).toBeGreaterThan(10);
    expect(biggestFrameShare(start, frames, target)).toBeLessThan(0.15);
    for (let i = 1; i < frames.length; i++) {
      expect((frames[i] as Camera).k).toBeGreaterThanOrEqual((frames[i - 1] as Camera).k);
    }
    expect(camera).toEqual({ x: target.x, y: target.y, k: target.k });
  });

  it("zooms in and out at the same pace", () => {
    const zoomIn = { x: 0, y: 0, k: 1 };
    const inFrames = glide(zoomIn, zoomTargetAt(zoomIn, null, 0, 0, Math.log(3)));
    const zoomOut = { x: 0, y: 0, k: 3 };
    const outFrames = glide(zoomOut, zoomTargetAt(zoomOut, null, 0, 0, -Math.log(3)));
    expect(Math.abs(inFrames.length - outFrames.length)).toBeLessThanOrEqual(1);
  });

  it("takes the same time at 60 Hz and 144 Hz", () => {
    const a = { x: 0, y: 0, k: 1 };
    const b = { x: 0, y: 0, k: 1 };
    const secondsAt60 = glide(a, zoomTargetAt(a, null, 0, 0, 1), 60).length / 60;
    const secondsAt144 = glide(b, zoomTargetAt(b, null, 0, 0, 1), 144).length / 144;
    expect(Math.abs(secondsAt60 - secondsAt144)).toBeLessThan(0.03);
  });

  it("keeps moving smoothly through a run of quick notches", () => {
    const camera = { x: 0, y: 0, k: 1 };
    const velocity = zeroVelocity();
    const amount = wheelZoomAmount(notch(false), "mouse");
    let target: CameraTarget | null = null;
    const ks: number[] = [camera.k];
    // Five notches 50 ms apart (3 frames each), then let it settle.
    for (let n = 0; n < 5; n++) {
      target = zoomTargetAt(camera, target, 0, 0, amount);
      for (let f = 0; f < 3; f++) {
        stepCamera(camera, velocity, target, 1 / 60);
        ks.push(camera.k);
      }
    }
    for (const f of glide(camera, target as CameraTarget, 60, velocity)) ks.push(f.k);
    // Per-frame zoom speed never jumps when a new notch lands.
    const speeds = ks.slice(1).map((k, i) => Math.log(k / (ks[i] as number)));
    for (let i = 1; i < speeds.length; i++) {
      expect(Math.abs((speeds[i] as number) - (speeds[i - 1] as number))).toBeLessThan(0.02);
    }
    expect(camera.k).toBeCloseTo(NOTCH_ZOOM ** 5, 6);
  });

  it("keeps the point under the pointer still on every frame", () => {
    const camera = { x: 0, y: 0, k: 0.7 };
    const before = worldAt(camera, -200, 150);
    const target = zoomTargetAt(camera, null, -200, 150, 2.4);
    for (const frame of glide(camera, target)) {
      const w = worldAt(frame, -200, 150);
      expect(w.x).toBeCloseTo(before.x, 6);
      expect(w.y).toBeCloseTo(before.y, 6);
    }
  });

  it("adds quick steps together rather than restarting from the camera", () => {
    const camera = { x: 0, y: 0, k: 1 };
    let target = zoomTargetAt(camera, null, 0, 0, 0.5);
    stepCamera(camera, zeroVelocity(), target, 1 / 60);
    target = zoomTargetAt(camera, target, 0, 0, 0.5);
    expect(target.k).toBeCloseTo(Math.exp(1), 6);
  });

  it("respects the scale limits without overshooting", () => {
    const camera = { x: 0, y: 0, k: 4 };
    const target = zoomTargetAt(camera, null, 30, 30, 100);
    expect(target.k).toBe(MAX_SCALE);
    for (const f of glide(camera, target)) expect(f.k).toBeLessThanOrEqual(MAX_SCALE + 1e-9);
  });

  it("carries a zoom glide along when panning mid-way", () => {
    const camera = { x: 0, y: 0, k: 1 };
    const velocity = zeroVelocity();
    const target = zoomTargetAt(camera, null, 100, 0, 1);
    stepCamera(camera, velocity, target, 1 / 60);
    panBy(camera, target, 40, -20);
    const expected = { x: target.x, y: target.y, k: target.k };
    glide(camera, target, 60, velocity);
    expect(camera).toEqual(expected);
  });
});
