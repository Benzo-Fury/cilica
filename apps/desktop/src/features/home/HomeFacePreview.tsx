/**
 * The face Settings is about to keep, drawn small with this week's numbers. Random cycles through
 * them. Faces without numbers yet borrow sample ones, and the caption says so.
 */
import { useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";
import { useSafeForWork } from "@/features/settings/hooks";
import type { HomeFace } from "@/ipc/types";
import { ScaledPreview } from "@/ui";
import { FaceView } from "./faces";
import { type GlanceFace, isJarFace, jarFaceReady, useHomeGlance } from "./glance";
import { sampleJar, withSamples } from "./sample";

const CYCLE: GlanceFace[] = [
  "tics",
  "overheard",
  "talk",
  "pace",
  "silence",
  "overtime",
  "slow_decision",
  "monologue",
  "own_words",
  "writing",
  "questions",
  "bare_note",
  "mid_sentence",
  "spelling",
  "ghostwriter",
  "undos",
  "swear_jar",
  "whos_paying",
  "clean_week",
  "doodle",
];
const CYCLE_MS = 3200;
const WRITTEN = new Set<GlanceFace>([
  "own_words",
  "writing",
  "questions",
  "bare_note",
  "mid_sentence",
  "spelling",
  "ghostwriter",
  "undos",
]);

export function HomeFacePreview({ kept }: { kept: HomeFace }) {
  const { data } = useHomeGlance(true);
  const reduced = useReducedMotion();
  const [step, setStep] = useState(0);
  const sfw = useSafeForWork();
  // Safe for work puts the jar away: its faces drop out of the cycle
  const cycle = sfw ? CYCLE.filter((f) => !isJarFace(f)) : CYCLE;
  const keptFace = sfw && isJarFace(kept) ? "random" : kept;
  useEffect(() => {
    if (keptFace !== "random" || reduced) return;
    const t = setInterval(() => setStep((s) => s + 1), CYCLE_MS);
    return () => clearInterval(t);
  }, [keptFace, reduced]);

  const face: GlanceFace =
    keptFace === "random" ? (cycle[step % cycle.length] ?? "doodle") : keptFace;
  const samples = withSamples(data);
  const sampled = samples.sampled;
  const real = face === "doodle" || !sampledFor(face, data);
  // The jar faces want different jars: a full one, or an empty one after a full week
  const glance =
    isJarFace(face) && !real
      ? {
          ...samples.glance,
          swear_jar: face === "clean_week" ? sampleJar(0, 17) : sampleJar(23, 17),
        }
      : samples.glance;
  const caption =
    keptFace === "random"
      ? `A different one each time you open Home.${sampled ? " Some here are samples until there is enough to show." : ""}`
      : real
        ? "This week's numbers."
        : face === "clean_week"
          ? "A sample. This one shows from Thursday when the jar is still empty after a week with coins in it."
          : isJarFace(face)
            ? "A sample. Until a few swears go in the jar, Home shows another face."
            : WRITTEN.has(face)
              ? "A sample. Until your notes give this one something to show, Home shows another face."
              : "Sample numbers. Until a meeting gives this one something to count, Home shows another face.";

  return (
    <ScaledPreview label={`Preview of Home: ${face}`} caption={caption}>
      <FaceView key={`${face}-${step}`} face={face} glance={glance} />
    </ScaledPreview>
  );
}

/** Whether this face has no real numbers yet. */
function sampledFor(face: GlanceFace, real: ReturnType<typeof useHomeGlance>["data"]): boolean {
  switch (face) {
    case "overheard":
      return !real?.quotes.length;
    case "own_words":
      return !real?.own_lines.length;
    case "questions":
      return !real?.questions.length;
    case "doodle":
      return false;
    case "swear_jar":
    case "whos_paying":
    case "clean_week":
      return !jarFaceReady(face, real?.swear_jar, { anyDay: true });
    default:
      return !real?.[face];
  }
}
