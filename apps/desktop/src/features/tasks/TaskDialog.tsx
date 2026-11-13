/** Create or edit a scheduled task: name, prompt, scope and schedule, on or off. */
import { useEffect, useId, useState } from "react";
import { useCategories } from "@/features/notes/hooks";
import { toAppError } from "@/ipc/client";
import type { Task } from "@/ipc/types";
import {
  Button,
  DialogActions,
  DialogRoot,
  DialogSurface,
  DialogTitle,
  Field,
  FormField,
  SegmentedControl,
  SelectMenu,
  Switch,
  TextArea,
} from "@/ui";
import { useSaveTask } from "./hooks";
import {
  draftFromForm,
  type FormErrors,
  formFromTask,
  type ScheduleKind,
  type ScopeKind,
  type TaskForm,
  validateForm,
  WEEKDAYS,
} from "./schedule";

const SCOPES = [
  { value: "all_notes", label: "All notes" },
  { value: "category", label: "A category" },
  { value: "last_days", label: "Recent" },
] as const satisfies readonly { value: ScopeKind; label: string }[];

const SCHEDULES = [
  { value: "daily", label: "Daily" },
  { value: "weekly", label: "Weekly" },
  { value: "every_n_days", label: "Every few days" },
] as const satisfies readonly { value: ScheduleKind; label: string }[];

export function TaskDialog({
  open,
  task,
  onOpenChange,
}: {
  open: boolean;
  /** `null` creates a new task. */
  task: Task | null;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <DialogRoot open={open} onOpenChange={onOpenChange}>
      {open && <TaskFormBody task={task} onDone={() => onOpenChange(false)} />}
    </DialogRoot>
  );
}

function TaskFormBody({ task, onDone }: { task: Task | null; onDone: () => void }) {
  const id = useId();
  const { data: categories = [] } = useCategories();
  const save = useSaveTask();
  const [form, setForm] = useState<TaskForm>(() => formFromTask(task));
  const [errors, setErrors] = useState<FormErrors>({});
  const [serverError, setServerError] = useState<string | null>(null);
  const set = <K extends keyof TaskForm>(key: K, value: TaskForm[K]) =>
    setForm((f) => ({ ...f, [key]: value }));

  // A category scope needs a category: preselect the first one.
  useEffect(() => {
    const first = categories[0];
    if (form.scope === "category" && !form.categoryId && first) {
      setForm((f) => ({ ...f, categoryId: first.id }));
    }
  }, [form.scope, form.categoryId, categories]);

  const submit = () => {
    const found = validateForm(form);
    setErrors(found);
    if (Object.keys(found).length) return;
    setServerError(null);
    save.mutate(
      { taskId: task?.id ?? null, draft: draftFromForm(form) },
      { onSuccess: onDone, onError: (error) => setServerError(toAppError(error).message) },
    );
  };

  return (
    <DialogSurface className="nt-dialog-wide nt-task-dialog">
      <DialogTitle>{task ? `Edit ${task.name}` : "New scheduled task"}</DialogTitle>
      <form
        className="nt-task-form"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <FormField label="Name" htmlFor={`${id}-name`}>
          <Field
            id={`${id}-name`}
            value={form.name}
            placeholder="Weekly digest"
            error={errors.name}
            autoFocus
            onChange={(e) => set("name", e.target.value)}
          />
        </FormField>
        <FormField
          label="What Cilica does"
          htmlFor={`${id}-prompt`}
          hint={errors.prompt ?? "Cilica only suggests changes; you approve them in the report."}
        >
          <TextArea
            id={`${id}-prompt`}
            value={form.prompt}
            rows={4}
            placeholder="Summarise what changed in my notes this week and list open action items with owners."
            aria-invalid={errors.prompt ? true : undefined}
            onChange={(e) => set("prompt", e.target.value)}
          />
        </FormField>
        <FormField label="Looks at">
          <div className="nt-form-row">
            <SegmentedControl
              label="Looks at"
              options={SCOPES}
              value={form.scope}
              onChange={(v) => set("scope", v)}
            />
            {form.scope === "category" &&
              (categories.length ? (
                <SelectMenu
                  label="Category"
                  value={form.categoryId ?? ""}
                  options={categories.map((c) => ({ value: c.id, label: c.name }))}
                  onChange={(v) => set("categoryId", v)}
                />
              ) : (
                <span className="nt-form-inline-label">Create a category first.</span>
              ))}
            {form.scope === "last_days" && (
              <>
                <span className="nt-form-inline-label">The last</span>
                <Field
                  size="sm"
                  type="number"
                  min={1}
                  max={365}
                  aria-label="Number of days"
                  value={String(form.lastDays)}
                  onChange={(e) => set("lastDays", Number(e.target.value))}
                />
                <span className="nt-form-inline-label">days</span>
              </>
            )}
          </div>
          {(errors.category || (form.scope === "last_days" && errors.days)) && (
            <p className="nt-field-msg nt-field-msg-error">{errors.category ?? errors.days}</p>
          )}
        </FormField>
        <FormField label="Runs">
          <div className="nt-form-row">
            <SegmentedControl
              label="Runs"
              options={SCHEDULES}
              value={form.schedule}
              onChange={(v) => set("schedule", v)}
            />
            {form.schedule === "weekly" && (
              <SelectMenu
                label="Day"
                value={String(form.weekday)}
                options={WEEKDAYS.map((d, i) => ({ value: String(i), label: `On ${d}` }))}
                onChange={(v) => set("weekday", Number(v))}
              />
            )}
            {form.schedule === "every_n_days" && (
              <>
                <span className="nt-form-inline-label">Every</span>
                <Field
                  size="sm"
                  type="number"
                  min={1}
                  max={90}
                  aria-label="Every how many days"
                  value={String(form.everyDays)}
                  onChange={(e) => set("everyDays", Number(e.target.value))}
                />
                <span className="nt-form-inline-label">days</span>
              </>
            )}
            <span className="nt-form-inline-label">at</span>
            <Field
              size="sm"
              className="nt-time-field"
              inputMode="numeric"
              maxLength={5}
              placeholder="09:00"
              aria-label="Time, 24-hour"
              value={form.time}
              onChange={(e) => set("time", e.target.value)}
            />
          </div>
          {(errors.time || (form.schedule === "every_n_days" && errors.days)) && (
            <p className="nt-field-msg nt-field-msg-error">{errors.time ?? errors.days}</p>
          )}
        </FormField>
        <div className="nt-form-switch">
          <span id={`${id}-on`} className="nt-form-label">
            Run on schedule
          </span>
          <Switch
            aria-labelledby={`${id}-on`}
            checked={form.enabled}
            onCheckedChange={(v) => set("enabled", v)}
          />
        </div>
        {serverError && (
          <p className="nt-field-msg nt-field-msg-error" role="alert">
            {serverError}
          </p>
        )}
        <DialogActions>
          <Button variant="secondary" onClick={onDone}>
            Cancel
          </Button>
          <Button type="submit" variant="primary" loading={save.isPending}>
            {task ? "Save task" : "Create task"}
          </Button>
        </DialogActions>
      </form>
    </DialogSurface>
  );
}
