import { describe, expect, it } from "vitest";
import type { Task } from "@/ipc/types";
import {
  defaultForm,
  describeSchedule,
  describeScope,
  draftFromForm,
  formFromTask,
  parseTime,
  validateForm,
} from "./schedule";

const task: Task = {
  id: "t",
  name: "Notes review",
  prompt: "Review my notes",
  scope: { type: "last_days", days: 14 },
  schedule: { type: "every_n_days", days: 3, hour: 9, minute: 5 },
  next_run_at: null,
  enabled: true,
  is_default: true,
  last_run: null,
  pending_count: 0,
};

describe("schedule form", () => {
  it("round-trips a task through the form", () => {
    const form = formFromTask(task);
    expect(form).toMatchObject({
      scope: "last_days",
      lastDays: 14,
      schedule: "every_n_days",
      everyDays: 3,
      time: "09:05",
    });
    expect(draftFromForm(form)).toEqual({
      name: "Notes review",
      prompt: "Review my notes",
      enabled: true,
      scope: { type: "last_days", days: 14 },
      schedule: { type: "every_n_days", days: 3, hour: 9, minute: 5 },
    });
  });

  it("maps every schedule and scope kind", () => {
    const base = { ...defaultForm(), name: " Digest ", prompt: " Sum up " };
    expect(
      draftFromForm({ ...base, schedule: "weekly", weekday: 4, time: "16:30" }).schedule,
    ).toEqual({
      type: "weekly",
      weekday: 4,
      hour: 16,
      minute: 30,
    });
    expect(draftFromForm({ ...base, schedule: "daily", time: "7:00" }).schedule).toEqual({
      type: "daily",
      hour: 7,
      minute: 0,
    });
    expect(draftFromForm({ ...base, scope: "category", categoryId: "c1" }).scope).toEqual({
      type: "category",
      category_id: "c1",
    });
    expect(draftFromForm(base)).toMatchObject({
      name: "Digest",
      prompt: "Sum up",
      scope: { type: "all_notes" },
    });
    const weekly = formFromTask({
      ...task,
      schedule: { type: "weekly", weekday: 6, hour: 18, minute: 0 },
      scope: { type: "category", category_id: "c2" },
    });
    expect(weekly).toMatchObject({
      schedule: "weekly",
      weekday: 6,
      scope: "category",
      categoryId: "c2",
    });
  });

  it("validates what the backend would reject", () => {
    expect(validateForm(defaultForm())).toMatchObject({
      name: expect.any(String),
      prompt: expect.any(String),
    });
    const ok = { ...defaultForm(), name: "a", prompt: "b" };
    expect(validateForm(ok)).toEqual({});
    expect(validateForm({ ...ok, time: "25:00" }).time).toBeDefined();
    expect(validateForm({ ...ok, scope: "category" }).category).toBeDefined();
    expect(validateForm({ ...ok, schedule: "every_n_days", everyDays: 0 }).days).toBeDefined();
    expect(parseTime("9:60")).toBeNull();
  });

  it("describes schedules and scopes in plain words", () => {
    expect(describeSchedule(task.schedule)).toBe("Every 3 days at 09:05");
    expect(describeSchedule({ type: "weekly", weekday: 0, hour: 16, minute: 0 })).toBe(
      "Mondays at 16:00",
    );
    expect(describeSchedule({ type: "daily", hour: 8, minute: 30 })).toBe("Daily at 08:30");
    expect(describeScope({ type: "last_days", days: 7 }, [])).toBe("The last 7 days");
    expect(
      describeScope({ type: "category", category_id: "x" }, [
        { id: "x", name: "Product", position: 1, collapsed: false },
      ]),
    ).toBe("Product");
  });
});
