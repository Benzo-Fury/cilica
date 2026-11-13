/** Scheduled tasks and their runs. A run finishes about 2 seconds after `runTaskNow`. */
import type { Task, TaskDraft, TaskRun, TaskRunDetail, TaskSchedule } from "@/ipc/types";
import { seedTaskProposals } from "./seed";
import type { MockContext } from "./state";
import { Timers } from "./timers";
import { appError, iso, newId, notFound } from "./util";

const RUN_MS = 2000;

/** The next wall-clock time a schedule fires, after `from`. */
export function nextRunAt(schedule: TaskSchedule, from: number = Date.now()): string {
  const at = new Date(from);
  at.setSeconds(0, 0);
  at.setHours(schedule.hour, schedule.minute);
  switch (schedule.type) {
    case "daily":
      if (at.getTime() <= from) at.setDate(at.getDate() + 1);
      break;
    case "weekly": {
      // 0 = Monday … 6 = Sunday; JS getDay() has 0 = Sunday.
      const today = (at.getDay() + 6) % 7;
      let days = (schedule.weekday - today + 7) % 7;
      if (days === 0 && at.getTime() <= from) days = 7;
      at.setDate(at.getDate() + days);
      break;
    }
    case "every_n_days":
      at.setDate(at.getDate() + Math.max(1, schedule.days));
      break;
  }
  return iso(at.getTime());
}

export function tasksCommands({ state, emit }: MockContext) {
  const timers = new Timers();

  const requireTask = (taskId: string): Task => {
    const task = state.tasks.find((t) => t.id === taskId);
    if (!task) throw notFound("task");
    return task;
  };

  const validate = (draft: TaskDraft) => {
    if (!draft.name.trim()) throw appError("invalid", "Give the task a name.");
    if (!draft.prompt.trim()) throw appError("invalid", "Tell Cilica what the task should do.");
    if (draft.scope.type === "category" && !state.categories.has(draft.scope.category_id)) {
      throw appError("not_found", "That category no longer exists. Pick another scope.");
    }
  };

  /** A task as the UI sees it: with its last run and pending proposal count. */
  const view = (task: Task): Task => {
    const runs = state.taskRuns
      .filter((r) => r.task_id === task.id)
      .sort((a, b) => b.started_at.localeCompare(a.started_at));
    const runIds = new Set(runs.map((r) => r.id));
    return {
      ...task,
      last_run: runs[0] ?? null,
      pending_count: state.proposals.filter(
        (p) => p.status === "pending" && p.task_run_id !== null && runIds.has(p.task_run_id),
      ).length,
    };
  };

  return {
    listTasks(): Task[] {
      return state.tasks.map(view);
    },

    createTask(draft: TaskDraft): Task {
      validate(draft);
      const task: Task = {
        id: newId("task"),
        name: draft.name.trim(),
        prompt: draft.prompt.trim(),
        scope: draft.scope,
        schedule: draft.schedule,
        next_run_at: draft.enabled ? nextRunAt(draft.schedule) : null,
        enabled: draft.enabled,
        is_default: false,
        last_run: null,
        pending_count: 0,
      };
      state.tasks.push(task);
      return view(task);
    },

    updateTask(taskId: string, draft: TaskDraft): Task {
      const task = requireTask(taskId);
      validate(draft);
      Object.assign(task, {
        name: draft.name.trim(),
        prompt: draft.prompt.trim(),
        scope: draft.scope,
        schedule: draft.schedule,
        enabled: draft.enabled,
        next_run_at: draft.enabled ? nextRunAt(draft.schedule) : null,
      });
      return view(task);
    },

    deleteTask(taskId: string): null {
      const task = requireTask(taskId);
      if (task.is_default) {
        throw appError("invalid", "Notes review is built in. Turn it off instead of deleting it.");
      }
      state.tasks = state.tasks.filter((t) => t.id !== taskId);
      return null;
    },

    runTaskNow(taskId: string): TaskRun {
      const task = requireTask(taskId);
      if (state.recording.phase !== "idle") {
        throw appError(
          "unavailable",
          "Scheduled tasks don't run while Cilica is recording. Try again after the meeting.",
        );
      }
      const run: TaskRun = {
        id: newId("run"),
        task_id: task.id,
        started_at: iso(),
        finished_at: null,
        status: "running",
        summary: "",
        error: null,
      };
      state.taskRuns.push(run);
      timers.later(RUN_MS, () => {
        run.status = "succeeded";
        run.finished_at = iso();
        run.summary = "Found 2 notes that cover the same topic and 1 note that looks stale.";
        state.proposals.unshift(...seedTaskProposals(run, Date.now(), state));
        task.next_run_at = task.enabled ? nextRunAt(task.schedule) : null;
        emit({ type: "task_run_finished", run: { ...run } });
        emit({ type: "data_changed", scopes: ["tasks", "proposals"], note_id: null });
        emit({
          type: "toast",
          toast: {
            kind: "action",
            title: `${task.name} finished`,
            description: "2 suggestions are waiting for you.",
            action_label: "Review",
            action: { type: "open_task_run", task_run_id: run.id },
          },
        });
      });
      return run;
    },

    listTaskRuns(taskId: string): TaskRun[] {
      requireTask(taskId);
      return state.taskRuns
        .filter((r) => r.task_id === taskId)
        .sort((a, b) => b.started_at.localeCompare(a.started_at));
    },

    getTaskRun(taskRunId: string): TaskRunDetail {
      const run = state.taskRuns.find((r) => r.id === taskRunId);
      if (!run) throw notFound("task run");
      return { run, proposals: state.proposals.filter((p) => p.task_run_id === run.id) };
    },

    dispose(): void {
      timers.clear();
    },
  };
}
