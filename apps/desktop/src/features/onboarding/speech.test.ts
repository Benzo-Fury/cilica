import { describe, expect, it } from "vitest";
import { defaultSttModels } from "@/ipc/mock/state";
import { orderModels } from "./SpeechStep";

describe("speech step", () => {
  it("offers every model, the recommended one first", () => {
    const models = defaultSttModels();
    const ordered = orderModels(models);
    expect(ordered).toHaveLength(models.length);
    expect(ordered[0]?.recommended).toBe(true);
    expect(ordered.slice(1).map((m) => m.id)).toEqual(
      models.filter((m) => !m.recommended).map((m) => m.id),
    );
  });
});
