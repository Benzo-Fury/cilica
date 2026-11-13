import { describe, expect, it } from "vitest";
import {
  firstWordsLabel,
  hasLiveSpeakers,
  speakerAdvice,
  speakerLabels,
  speakerSourceHint,
} from "./speech";

describe("transcription test", () => {
  it("says how quickly the first words came back", () => {
    expect(firstWordsLabel(640)).toBe("First words after 0.6 s.");
    expect(firstWordsLabel(1_250)).toBe("First words after 1.3 s.");
    expect(firstWordsLabel(null)).toBe("");
  });
});

describe("live speakers", () => {
  it("only services that label speakers live offer it", () => {
    expect(hasLiveSpeakers("soniox")).toBe(true);
    expect(hasLiveSpeakers("deepgram")).toBe(true);
    expect(hasLiveSpeakers("assembly_ai")).toBe(true);
    expect(hasLiveSpeakers("open_ai")).toBe(false);
    expect(hasLiveSpeakers("open_ai_compatible")).toBe(false);
    expect(hasLiveSpeakers("whisper_local")).toBe(false);
  });

  it("falls back to this computer when the service can't label speakers", () => {
    expect(speakerLabels("service", "soniox")).toBe("service");
    expect(speakerLabels("service", "open_ai")).toBe("local");
    expect(speakerLabels("service", "whisper_local")).toBe("local");
    expect(speakerLabels("off", "soniox")).toBe("off");
  });

  it("explains each way of labelling speakers", () => {
    expect(speakerSourceHint("local", "soniox")).toContain("Pending");
    expect(speakerSourceHint("service", "deepgram")).toContain("$0.12 an hour");
    expect(speakerSourceHint("off", "whisper_local")).toContain("You for your microphone");
  });

  it("recommends this computer only where it keeps up", () => {
    expect(speakerAdvice(true, "soniox")).toMatch(/^Recommended: this computer/);
    expect(speakerAdvice(false, "soniox")).toContain("Soniox's labels are the lighter choice");
    expect(speakerAdvice(false, "whisper_local")).toContain("Turn it off if recording feels slow");
    expect(speakerAdvice(false, "soniox")).not.toContain("Recommended");
  });
});
