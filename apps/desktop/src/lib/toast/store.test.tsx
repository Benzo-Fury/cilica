import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createToastStore } from "./store";
import { Toaster } from "./Toaster";
import { dismissNewestToast, toast, toastStore, triggerLatestUndo } from "./toast";

const visible = (store: ReturnType<typeof createToastStore>) =>
  store
    .getSnapshot()
    .toasts.filter((t) => t.status === "visible")
    .map((t) => t.title);
const queued = (store: ReturnType<typeof createToastStore>) =>
  store
    .getSnapshot()
    .toasts.filter((t) => t.status === "queued")
    .map((t) => t.title);

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  toastStore.dismiss();
  vi.useRealTimers();
});

describe("durations", () => {
  it.each([
    ["default", 4000],
    ["success", 4000],
    ["error", 8000],
    ["action", 6000],
    ["undo", 5000],
  ] as const)("%s toasts leave after %ims", (variant, ms) => {
    const store = createToastStore();
    store.add({ title: "Hello", variant });
    vi.advanceTimersByTime(ms - 1);
    expect(visible(store)).toEqual(["Hello"]);
    vi.advanceTimersByTime(1);
    expect(visible(store)).toEqual([]);
  });

  it("keeps loading toasts until they settle, and honours a custom duration", () => {
    const store = createToastStore();
    store.add({ title: "Working", variant: "loading" });
    store.add({ title: "Sticky", duration: null });
    store.add({ title: "Quick", duration: 500 });
    vi.advanceTimersByTime(60_000);
    expect(visible(store)).toEqual(["Working", "Sticky"]);
  });

  it("calls onDismiss when the timer runs out", () => {
    const store = createToastStore();
    const onDismiss = vi.fn();
    store.add({ title: "Bye", onDismiss });
    vi.advanceTimersByTime(4000);
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});

describe("queue", () => {
  it("shows at most three and promotes the next in order", () => {
    const store = createToastStore();
    const ids = ["A", "B", "C", "D", "E"].map((title) => store.add({ title, duration: null }));
    expect(visible(store)).toEqual(["A", "B", "C"]);
    expect(queued(store)).toEqual(["D", "E"]);
    store.dismiss(ids[1]);
    expect(visible(store)).toEqual(["A", "C", "D"]);
    expect(queued(store)).toEqual(["E"]);
  });

  it("starts a queued toast's timer only when it becomes visible", () => {
    const store = createToastStore();
    const first = store.add({ title: "A", duration: null });
    store.add({ title: "B", duration: null });
    store.add({ title: "C", duration: null });
    store.add({ title: "D" });
    vi.advanceTimersByTime(10_000);
    expect(queued(store)).toEqual(["D"]);
    store.dismiss(first);
    expect(visible(store)).toContain("D");
    vi.advanceTimersByTime(3999);
    expect(visible(store)).toContain("D");
    vi.advanceTimersByTime(1);
    expect(visible(store)).not.toContain("D");
  });

  it("dismisses everything without an id", () => {
    const store = createToastStore();
    for (const title of ["A", "B", "C", "D"]) store.add({ title });
    store.dismiss();
    expect(store.getSnapshot().toasts).toEqual([]);
  });
});

describe("pause and resume", () => {
  it("keeps the remaining time across a pause", () => {
    const store = createToastStore();
    store.add({ title: "Saved" });
    vi.advanceTimersByTime(3000);
    store.pause("hover");
    expect(store.getSnapshot().paused).toBe(true);
    vi.advanceTimersByTime(60_000);
    expect(visible(store)).toEqual(["Saved"]);
    store.resume("hover");
    vi.advanceTimersByTime(999);
    expect(visible(store)).toEqual(["Saved"]);
    vi.advanceTimersByTime(1);
    expect(visible(store)).toEqual([]);
  });

  it("stays paused until every reason is released", () => {
    const store = createToastStore();
    store.add({ title: "Saved" });
    store.pause("hover");
    store.pause("blur");
    store.resume("hover");
    vi.advanceTimersByTime(10_000);
    expect(visible(store)).toEqual(["Saved"]);
    store.resume("blur");
    vi.advanceTimersByTime(4000);
    expect(visible(store)).toEqual([]);
  });

  it("does not start timers for toasts promoted while paused", () => {
    const store = createToastStore({ maxVisible: 1 });
    const a = store.add({ title: "A", duration: null });
    store.add({ title: "B" });
    store.pause("hover");
    store.dismiss(a);
    vi.advanceTimersByTime(10_000);
    expect(visible(store)).toEqual(["B"]);
    store.resume("hover");
    vi.advanceTimersByTime(4000);
    expect(visible(store)).toEqual([]);
  });

  it("reports elapsed time for the countdown ring", () => {
    const store = createToastStore();
    store.add({ title: "Moved", variant: "undo" });
    vi.advanceTimersByTime(2000);
    const t = store.getSnapshot().toasts[0];
    expect(t && store.elapsed(t)).toBe(2000);
  });
});

describe("update in place", () => {
  it("replaces content by id and restarts the timer when the variant changes", () => {
    const store = createToastStore();
    const id = store.add({ title: "Saving", variant: "loading" });
    vi.advanceTimersByTime(10_000);
    store.add({ id, title: "Saved", variant: "success" });
    const snap = store.getSnapshot().toasts;
    expect(snap).toHaveLength(1);
    expect(snap[0]?.title).toBe("Saved");
    expect(snap[0]?.version).toBe(1);
    vi.advanceTimersByTime(4000);
    expect(store.getSnapshot().toasts).toHaveLength(0);
  });

  it("merges a title-only update without touching the timer", () => {
    const store = createToastStore();
    const id = store.add({ title: "One", description: "Kept" });
    vi.advanceTimersByTime(3000);
    store.update(id, { title: "Two" });
    expect(store.getSnapshot().toasts[0]).toMatchObject({ title: "Two", description: "Kept" });
    vi.advanceTimersByTime(1000);
    expect(store.getSnapshot().toasts).toHaveLength(0);
  });
});

describe("toast API", () => {
  it("creates each variant", () => {
    toast("Saved");
    toast.success("Summary ready", { description: "Acme kickoff" });
    toast.error("Mic unavailable", { description: "Choose another input in Settings." });
    const variants = toastStore.getSnapshot().toasts.map((t) => t.variant);
    expect(variants).toEqual(["default", "success", "error"]);
  });

  it("runs an action's onClick", () => {
    const onClick = vi.fn();
    toast.action("Zoom meeting detected", { action: { label: "Record", onClick } });
    const t = toastStore.getSnapshot().toasts[0];
    t?.action?.onClick();
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("moves a promise from loading to success with a formatter", async () => {
    let resolve: (value: number) => void = () => {};
    const task = new Promise<number>((r) => {
      resolve = r;
    });
    toast.promise(task, {
      loading: "Summarising…",
      success: (n) => ({ title: "Summary ready", description: `${n} action items` }),
      error: "Summary failed",
    });
    expect(toastStore.getSnapshot().toasts[0]).toMatchObject({
      variant: "loading",
      title: "Summarising…",
    });
    resolve(6);
    await vi.runAllTicks();
    await task;
    expect(toastStore.getSnapshot().toasts[0]).toMatchObject({
      variant: "success",
      title: "Summary ready",
      description: "6 action items",
    });
  });

  it("moves a promise from loading to error", async () => {
    const task = Promise.reject({ code: "upstream", message: "Anthropic did not answer." });
    const shown = toast.promise(task, {
      loading: "Summarising…",
      success: "Summary ready",
      error: (e) => ({ title: "Summary failed", description: (e as { message: string }).message }),
    });
    await expect(shown).rejects.toBeTruthy();
    expect(toastStore.getSnapshot().toasts[0]).toMatchObject({
      variant: "error",
      title: "Summary failed",
      description: "Anthropic did not answer.",
    });
  });

  it("dismisses the newest toast for Esc", () => {
    expect(dismissNewestToast()).toBe(false);
    toast("First");
    toast("Second");
    expect(dismissNewestToast()).toBe(true);
    expect(toastStore.getSnapshot().toasts.map((t) => t.title)).toEqual(["First"]);
  });

  it("runs the newest visible undo for ⌘Z", () => {
    const older = vi.fn();
    const newer = vi.fn();
    expect(triggerLatestUndo()).toBe(false);
    toast.undo("Moved to Trash", { onUndo: older });
    toast.undo("Renamed", { onUndo: newer });
    toast("Saved");
    expect(triggerLatestUndo()).toBe(true);
    expect(newer).toHaveBeenCalledOnce();
    expect(older).not.toHaveBeenCalled();
    expect(toastStore.getSnapshot().toasts.map((t) => t.title)).toEqual([
      "Moved to Trash",
      "Saved",
    ]);
  });
});

describe("<Toaster />", () => {
  it("renders visible toasts with the right roles", () => {
    const store = createToastStore();
    render(<Toaster store={store} />);
    act(() => {
      store.add({ title: "Copied" });
      store.add({ title: "Mic unavailable", variant: "error" });
    });
    expect(screen.getByRole("region", { name: "Notifications" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Copied");
    expect(screen.getByRole("alert")).toHaveTextContent("Mic unavailable");
  });

  it("grows into a card with its action after the pill lands", () => {
    const store = createToastStore();
    const onUndo = vi.fn();
    render(<Toaster store={store} />);
    act(() => {
      store.add({
        title: "Moved 3 notes to Trash",
        description: "They stay there for 30 days.",
        variant: "undo",
        onUndo,
      });
    });
    expect(screen.queryByText("They stay there for 30 days.")).not.toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(screen.getByText("They stay there for 30 days.")).toBeInTheDocument();
    act(() => {
      screen.getByRole("button", { name: "Undo" }).click();
    });
    expect(onUndo).toHaveBeenCalledOnce();
    expect(store.getSnapshot().toasts).toHaveLength(0);
  });
});
