/**
 * Which lines of a live transcript are new, for their entrance motion. New lines rise into
 * place. A pending line whose speaker Cilica works out is "arriving": its words glide from where
 * they were under Pending to their place under the speaker (see `useTranscriptFlip`), so they do
 * not rise in again. Nothing animates when the transcript first loads, or outside a recording.
 */
import { useLayoutEffect, useRef, useState } from "react";
import type { Segment } from "@/ipc/types";
import { segmentChanges } from "./model";

/** How long a line counts as new: long enough for its motion, short enough that scrolling back
 * to it later does not replay it. */
const FRESH_MS = 900;

export interface SegmentMotion {
  /** Lines that just appeared. */
  fresh: ReadonlySet<string>;
  /** Lines that just moved from "Pending" to their speaker. */
  arriving: ReadonlySet<string>;
}

const NONE: SegmentMotion = { fresh: new Set(), arriving: new Set() };

export function useSegmentMotion(
  segments: readonly Segment[],
  live: boolean,
  pendingSpeakers: boolean,
): SegmentMotion {
  const known = useRef<Map<string, Segment> | null>(null);
  const [motion, setMotion] = useState<SegmentMotion>(NONE);
  const timers = useRef(new Set<ReturnType<typeof setTimeout>>());

  useLayoutEffect(() => {
    const before = known.current;
    known.current = new Map(segments.map((s) => [s.id, s]));
    if (!before || !live) return;
    const { added, resolved } = segmentChanges(before, segments, pendingSpeakers);
    if (added.length === 0 && resolved.length === 0) return;
    const resolvedIds = resolved.map((s) => s.id);

    // Set before paint, so a moved line never shows without its motion.
    setMotion((m) => ({
      fresh: new Set([...m.fresh, ...added]),
      arriving: new Set([...m.arriving, ...resolvedIds]),
    }));
    const t = setTimeout(() => {
      timers.current.delete(t);
      setMotion((m) => ({
        fresh: new Set([...m.fresh].filter((id) => !added.includes(id))),
        arriving: new Set([...m.arriving].filter((id) => !resolvedIds.includes(id))),
      }));
    }, FRESH_MS);
    timers.current.add(t);
  }, [segments, live, pendingSpeakers]);

  useLayoutEffect(() => {
    if (!live) setMotion(NONE);
  }, [live]);

  useLayoutEffect(() => {
    const pending = timers.current;
    return () => {
      for (const t of pending) clearTimeout(t);
    };
  }, []);

  return motion;
}
