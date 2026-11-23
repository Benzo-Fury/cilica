import { describe, expect, it } from "vitest";
import type { RecordingState } from "@/ipc/types";
import { isFinishing, recorderMode, stopNeedsConfirm } from "./model";

const state = (patch: Partial<RecordingState>): RecordingState => ({
  phase: "idle",
  note_id: null,
  meeting_app: null,
  started_at: null,
  auto_stop_at: null,
  live_speakers: false,
  paused_at: null,
  paused_ms: 0,
  meeting_live: null,
  ...patch,
});

const NOW = Date.parse("2026-11-11T10:05:00Z");

describe("recorderMode", () => {
  it("maps each phase to what the recording chip shows", () => {
    expect(recorderMode(state({}), NOW)).toEqual({ kind: "idle" });
    expect(recorderMode(state({ phase: "starting" }), NOW)).toEqual({ kind: "starting" });
    for (const phase of ["stopping", "summarizing"] as const) {
      expect(recorderMode(state({ phase }), NOW)).toEqual({
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
      recorderMode(state({ phase: "recording", started_at: "2026-11-11T10:00:30Z" }), NOW),
    ).toEqual({ kind: "recording", elapsedMs: 270_000, paused: false, autoStopInMs: null });
  });

  it("holds the time while paused and leaves pauses out after", () => {
    const base = { phase: "recording" as const, started_at: "2026-11-11T10:00:00Z" };
    expect(
      recorderMode(state({ ...base, paused_at: "2026-11-11T10:03:00Z", paused_ms: 60_000 }), NOW),
    ).toMatchObject({ paused: true, elapsedMs: 120_000 });
    expect(recorderMode(state({ ...base, paused_ms: 90_000 }), NOW)).toMatchObject({
      paused: false,
      elapsedMs: 210_000,
    });
  });

  it("counts down to an auto-stop, never below zero", () => {
    const base = { phase: "recording" as const, started_at: "2026-11-11T10:00:00Z" };
    expect(
      recorderMode(state({ ...base, auto_stop_at: "2026-11-11T10:05:09Z" }), NOW),
    ).toMatchObject({
      autoStopInMs: 9000,
    });
    expect(
      recorderMode(state({ ...base, auto_stop_at: "2026-11-11T10:04:00Z" }), NOW),
    ).toMatchObject({
      autoStopInMs: 0,
    });
  });

  it("survives a missing or bad start time", () => {
    expect(recorderMode(state({ phase: "recording", started_at: "nonsense" }), NOW)).toMatchObject({
      elapsedMs: 0,
    });
  });
});

describe("stopNeedsConfirm", () => {
  it("asks only while a meeting in the recording is still in its call", () => {
    expect(stopNeedsConfirm(state({ phase: "recording", meeting_live: "Zoom" }))).toBe("Zoom");
    expect(stopNeedsConfirm(state({ phase: "recording", meeting_live: null }))).toBeNull();
    expect(stopNeedsConfirm(state({ phase: "recording", meeting_live: " " }))).toBeNull();
    expect(stopNeedsConfirm(state({ phase: "starting", meeting_live: "Zoom" }))).toBeNull();
    expect(stopNeedsConfirm(state({ phase: "stopping", meeting_live: "Zoom" }))).toBeNull();
  });
});
