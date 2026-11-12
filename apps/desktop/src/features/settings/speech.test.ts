import { describe, expect, it } from "vitest";
import { firstWordsLabel, hasLiveSpeakers, liveSpeakersHint } from "./speech";

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

  it("says when speakers will appear instead", () => {
    expect(liveSpeakersHint("open_ai", true)).toBe(
      "OpenAI can't tell people apart live. Speakers appear once the meeting is finished.",
    );
    expect(liveSpeakersHint("open_ai_compatible", false)).toBe(
      "Your transcription service can't tell people apart live.",
    );
    expect(liveSpeakersHint("deepgram", true)).toContain("$0.12 an hour");
  });
});
