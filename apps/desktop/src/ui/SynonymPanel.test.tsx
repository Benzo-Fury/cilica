import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SynonymPanel } from "./SynonymPanel";

describe("SynonymPanel", () => {
  it("shows every synonym as a pill under one label", () => {
    const onPick = vi.fn();
    render(
      <SynonymPanel
        word="holiday"
        words={["vacation", "break", "day off"]}
        onPick={onPick}
        onClose={() => {}}
      />,
    );
    expect(screen.getByText("Replace with")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "day off" }));
    expect(onPick).toHaveBeenCalledWith("day off");
  });

  it("says when there are none", () => {
    render(<SynonymPanel word="zorbly" words={[]} onPick={() => {}} onClose={() => {}} />);
    expect(screen.getByText("No synonyms for “zorbly”")).toBeVisible();
  });
});
