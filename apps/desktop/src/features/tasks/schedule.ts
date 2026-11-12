/**
 * The task editor's form and its mapping to `TaskDraft` (scope and schedule), plus the plain
 * words a task list uses for them ("Every 3 days at 09:00", "Last 7 days").
 */
import type { Category, Task, TaskDraft, TaskSchedule, TaskScope } from "@/ipc/types";

export type ScopeKind = TaskScope["type"];
export type ScheduleKind = TaskSchedule["type"];

export interface TaskForm {
  name: string;
  prompt: string;
  enabled: boolean;
  scope: ScopeKind;
  categoryId: string | null;
  lastDays: number;
  schedule: ScheduleKind;
  everyDays: number;
  /** 0 = Monday … 6 = Sunday. */
  weekday: number;
  /** "HH:MM", local time. */
  time: string;
}

export const WEEKDAYS = [
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
  "Sunday",
] as const;

const pad = (n: number) => String(n).padStart(2, "0");

export function parseTime(time: string): { hour: number; minute: number } | null {
  const m = time.trim().match(/^(\d{1,2}):(\d{2})$/);
  if (!m) return null;
  const hour = Number(m[1]);
  const minute = Number(m[2]);
  if (hour > 23 || minute > 59) return null;
  return { hour, minute };
}

export function formatTime(hour: number, minute: number): string {
  return `${pad(hour)}:${pad(minute)}`;
}

export const defaultForm = (): TaskForm => ({
  name: "",
  prompt: "",
  enabled: true,
  scope: "all_notes",
  categoryId: null,
  lastDays: 7,
  schedule: "weekly",
  everyDays: 3,
  weekday: 0,
  time: "09:00",
});

/** The form for an existing task, or a new one. */
export function formFromTask(task: Task | null): TaskForm {
  const form = defaultForm();
  if (!task) return form;
  form.name = task.name;
  form.prompt = task.prompt;
  form.enabled = task.enabled;
  form.scope = task.scope.type;
  if (task.scope.type === "category") form.categoryId = task.scope.category_id;
  if (task.scope.type === "last_days") form.lastDays = task.scope.days;
  form.schedule = task.schedule.type;
  form.time = formatTime(task.schedule.hour, task.schedule.minute);
  if (task.schedule.type === "every_n_days") form.everyDays = task.schedule.days;
  if (task.schedule.type === "weekly") form.weekday = task.schedule.weekday;
  return form;
}

export type FormErrors = Partial<Record<"name" | "prompt" | "time" | "category" | "days", string>>;

export function validateForm(form: TaskForm): FormErrors {
  const errors: FormErrors = {};
  if (!form.name.trim()) errors.name = "Give the task a name.";
  if (!form.prompt.trim()) errors.prompt = "Tell Margin what to do each time it runs.";
  if (!parseTime(form.time)) errors.time = "Use a 24-hour time like 09:00.";
  if (form.scope === "category" && !form.categoryId) errors.category = "Pick a category.";
  if (form.scope === "last_days" && !(form.lastDays >= 1 && form.lastDays <= 365))
    errors.days = "Pick between 1 and 365 days.";
  if (form.schedule === "every_n_days" && !(form.everyDays >= 1 && form.everyDays <= 90))
    errors.days = "Run it every 1 to 90 days.";
  return errors;
}

export function scopeOf(form: TaskForm): TaskScope {
  switch (form.scope) {
    case "category":
      return { type: "category", category_id: form.categoryId ?? "" };
    case "last_days":
      return { type: "last_days", days: Math.round(form.lastDays) };
    case "all_notes":
      return { type: "all_notes" };
  }
}

export function scheduleOf(form: TaskForm): TaskSchedule {
  const { hour, minute } = parseTime(form.time) ?? { hour: 9, minute: 0 };
  switch (form.schedule) {
    case "every_n_days":
      return { type: "every_n_days", days: Math.round(form.everyDays), hour, minute };
    case "weekly":
      return { type: "weekly", weekday: form.weekday, hour, minute };
    case "daily":
      return { type: "daily", hour, minute };
  }
}

export function draftFromForm(form: TaskForm): TaskDraft {
  return {
    name: form.name.trim(),
    prompt: form.prompt.trim(),
    enabled: form.enabled,
    scope: scopeOf(form),
    schedule: scheduleOf(form),
  };
}

/** "Every 3 days at 09:00", "Mondays at 16:00", "Daily at 08:30". */
export function describeSchedule(schedule: TaskSchedule): string {
  const at = formatTime(schedule.hour, schedule.minute);
  switch (schedule.type) {
    case "every_n_days":
      return schedule.days === 1 ? `Daily at ${at}` : `Every ${schedule.days} days at ${at}`;
    case "weekly":
      return `${WEEKDAYS[schedule.weekday] ?? "Monday"}s at ${at}`;
    case "daily":
      return `Daily at ${at}`;
  }
}

export function describeScope(scope: TaskScope, categories: readonly Category[]): string {
  switch (scope.type) {
    case "all_notes":
      return "All notes";
    case "category":
      return categories.find((c) => c.id === scope.category_id)?.name ?? "A deleted category";
    case "last_days":
      return scope.days === 1 ? "The last day" : `The last ${scope.days} days`;
  }
}
