/**
 * The cloud speech services Cilica supports, shared by Settings › Transcription and setup.
 */
import type { SttEngine } from "@/ipc/types";

export type CloudEngine = Exclude<SttEngine, "whisper_local">;

/** Cloud services, cheapest live option first. Prices are approximate, per hour of audio. */
export const CLOUD_PROVIDERS = [
  {
    value: "soniox",
    label: "Soniox",
    price: "About $0.12 an hour",
    hint: "About $0.12 an hour. Fast, 60+ languages.",
  },
  {
    value: "assembly_ai",
    label: "AssemblyAI",
    price: "About $0.15 an hour",
    hint: "About $0.15 an hour in English, $0.45 in other languages.",
  },
  {
    value: "deepgram",
    label: "Deepgram",
    price: "About $0.46 an hour",
    hint: "About $0.46 an hour, $0.55 for other languages. Nova-3.",
  },
  {
    value: "open_ai",
    label: "OpenAI",
    price: "About $0.27 an hour",
    hint: "About $0.27 an hour. Text appears after each pause.",
  },
  {
    value: "open_ai_compatible",
    label: "Other (OpenAI-compatible)",
    price: "Your own service",
    hint: "Groq, Fireworks, Together, Mistral or your own server. Text appears after each pause.",
  },
] as const satisfies readonly {
  value: CloudEngine;
  label: string;
  price: string;
  hint: string;
}[];

export function providerName(engine: SttEngine): string {
  if (engine === "open_ai_compatible") return "your transcription service";
  return CLOUD_PROVIDERS.find((p) => p.value === engine)?.label ?? "Whisper";
}

/** "First words after 0.6 s." for the transcription test, or nothing when none came. */
export function firstWordsLabel(ms: number | null): string {
  if (ms === null) return "";
  return `First words after ${(ms / 1000).toFixed(1)} s.`;
}

/** Services that can say who is speaking while the meeting is live (crates/domain: `has_live_speakers`). */
export function hasLiveSpeakers(engine: SttEngine): boolean {
  return engine === "soniox" || engine === "deepgram" || engine === "assembly_ai";
}

/** The description under "Show who's speaking live" for this engine. */
export function liveSpeakersHint(engine: SttEngine, identifyAfter: boolean): string {
  const after = identifyAfter ? " Speakers appear once the meeting is finished." : "";
  switch (engine) {
    case "soniox":
      return "Soniox labels each person as they talk, on the call and in the room.";
    case "deepgram":
      return "Deepgram labels each person as they talk, on the call and in the room. Adds about $0.12 an hour for each.";
    case "assembly_ai":
      return "AssemblyAI labels each person as they talk, on the call and in the room. In beta: very short turns may stay unlabelled.";
    case "whisper_local":
      return `Whisper on this computer can't tell people apart live.${after}`;
    default:
      return `${providerName(engine).replace(/^your/, "Your")} can't tell people apart live.${after}`;
  }
}
