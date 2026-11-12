/** Scheduled tasks: the list (sidebar badge, task page), runs and reports, and their edits. */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useBackendEvent } from "@/app/backendEvents";
import { qk } from "@/app/queryKeys";
import { api, errorMessage } from "@/ipc/client";
import type { Task, TaskDraft } from "@/ipc/types";
import { toast } from "@/lib/toast";

export function useTasks() {
  return useQuery({ queryKey: qk.tasks, queryFn: () => api.listTasks() });
}

export function useTaskRuns(taskId: string | null) {
  return useQuery({
    queryKey: qk.taskRuns(taskId ?? ""),
    queryFn: () => api.listTaskRuns(taskId as string),
    enabled: Boolean(taskId),
  });
}

export function useTaskRun(runId: string) {
  return useQuery({ queryKey: qk.taskRun(runId), queryFn: () => api.getTaskRun(runId) });
}

/** A finished run refreshes the list and its report (the toast comes from the backend). */
export function useTaskRunEvents() {
  const queryClient = useQueryClient();
  useBackendEvent("task_run_finished", ({ run }) => {
    void queryClient.invalidateQueries({ queryKey: qk.taskRun(run.id) });
    void queryClient.invalidateQueries({ queryKey: qk.taskRuns(run.task_id) });
  });
}

export function useSaveTask() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ taskId, draft }: { taskId: string | null; draft: TaskDraft }) =>
      taskId ? api.updateTask(taskId, draft) : api.createTask(draft),
    onSuccess: (task, { taskId }) => {
      queryClient.setQueryData<Task[]>(qk.tasks, (list) =>
        taskId ? list?.map((t) => (t.id === task.id ? task : t)) : [...(list ?? []), task],
      );
      toast(taskId ? `Saved ${task.name}` : `Created ${task.name}`, {
        description: task.enabled ? undefined : "It is off until you turn it on.",
      });
    },
  });
}

export function useToggleTask() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ task, enabled }: { task: Task; enabled: boolean }) =>
      api.updateTask(task.id, {
        name: task.name,
        prompt: task.prompt,
        scope: task.scope,
        schedule: task.schedule,
        enabled,
      }),
    onMutate: ({ task, enabled }) => {
      const previous = queryClient.getQueryData<Task[]>(qk.tasks);
      queryClient.setQueryData<Task[]>(qk.tasks, (list) =>
        list?.map((t) => (t.id === task.id ? { ...t, enabled } : t)),
      );
      return { previous };
    },
    onError: (error, _v, context) => {
      queryClient.setQueryData(qk.tasks, context?.previous);
      toast.error("The task was not changed", { description: errorMessage(error) });
    },
    onSuccess: (task) =>
      queryClient.setQueryData<Task[]>(qk.tasks, (list) =>
        list?.map((t) => (t.id === task.id ? task : t)),
      ),
  });
}

export function useDeleteTask() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (taskId: string) => api.deleteTask(taskId),
    onMutate: (taskId) => {
      const previous = queryClient.getQueryData<Task[]>(qk.tasks);
      queryClient.setQueryData<Task[]>(qk.tasks, (list) => list?.filter((t) => t.id !== taskId));
      return { previous };
    },
    onError: (error, _v, context) => {
      queryClient.setQueryData(qk.tasks, context?.previous);
      toast.error("The task was not deleted", { description: errorMessage(error) });
    },
    onSuccess: (_r, taskId, context) => {
      const name = context?.previous?.find((t) => t.id === taskId)?.name ?? "the task";
      toast(`Deleted ${name}`);
    },
  });
}

export function useRunTaskNow() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (taskId: string) => api.runTaskNow(taskId),
    onSuccess: (run) => {
      queryClient.setQueryData<Task[]>(qk.tasks, (list) =>
        list?.map((t) => (t.id === run.task_id ? { ...t, last_run: run } : t)),
      );
      void queryClient.invalidateQueries({ queryKey: qk.taskRuns(run.task_id) });
    },
    onError: (error) => toast.error("The task did not start", { description: errorMessage(error) }),
  });
}
