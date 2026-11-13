import { describe, expect, it } from "vitest";
import type { RecordingState } from "@/ipc/types";
import { capsuleMode, isFinishing } from "./model";

const state = (patch: Partial<RecordingState>): RecordingState => ({
  phase: "idle",
  note_id: null,
  meeting_app: null,
  started_at: null,
  auto_stop_at: null,
  live_speakers: false,
  ...patch,
});

const NOW = Date.parse("2026-11-11T10:05:00Z");

describe("capsuleMode", () => {
  it("maps each phase to what the capsule shows", () => {
    expect(capsuleMode(state({}), NOW)).toEqual({ kind: "idle" });
    expect(capsuleMode(state({ phase: "starting" }), NOW)).toEqual({ kind: "starting" });
    for (const phase of ["stopping", "summarizing"] as const) {
      expect(capsuleMode(state({ phase }), NOW)).toEqual({
        kind: "finishing",
        label: "Writing notes…",
        step: phase,
      });
      expect(isFinishing(state({ phase }))).toBe(true);
    }
    expect(isFinishing(state({ phase: "recording" }))).toBe(false);
  });

  it("times a recording from started_at", () => {
    expect(
      capsuleMode(state({ phase: "recording", started_at: "2026-11-11T10:00:30Z" }), NOW),
    ).toEqual({ kind: "recording", elapsedMs: 270_000, autoStopInMs: null });
  });

  it("counts down to an auto-stop, never below zero", () => {
    const base = { phase: "recording" as const, started_at: "2026-11-11T10:00:00Z" };
    expect(
      capsuleMode(state({ ...base, auto_stop_at: "2026-11-11T10:05:09Z" }), NOW),
    ).toMatchObject({
      autoStopInMs: 9000,
    });
    expect(
      capsuleMode(state({ ...base, auto_stop_at: "2026-11-11T10:04:00Z" }), NOW),
    ).toMatchObject({
      autoStopInMs: 0,
    });
  });

  it("survives a missing or bad start time", () => {
    expect(capsuleMode(state({ phase: "recording", started_at: "nonsense" }), NOW)).toMatchObject({
      elapsedMs: 0,
    });
  });
});
