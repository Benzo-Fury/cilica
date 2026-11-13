/**
 * Scheduled tasks (spec › Scheduled tasks): each task with its schedule, next run, a status dot
 * and its waiting suggestions; create and edit, turn on or off, run now. A run opens as a report.
 */

import { CalendarClock, Ellipsis, Pencil, Play, Plus, Trash2 } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useState } from "react";
import { ShortcutTip } from "@/app/keybinds";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { navigate } from "@/app/router";
import { useCategories } from "@/features/notes/hooks";
import { CanvasBand } from "@/features/shell/CanvasBand";
import type { Task } from "@/ipc/types";
import { dayAndTime, relativeDate } from "@/lib/format";
import {
  Button,
  ConfirmDialog,
  CountBadge,
  Crumb,
  EmptyState,
  IconButton,
  ListItem,
  Menu,
  MenuContent,
  MenuItem,
  MenuSeparator,
  MenuTrigger,
  PageColumn,
  ScrollArea,
  Shimmer,
  SkeletonText,
  StatusDot,
  type StatusTone,
  Switch,
  Tooltip,
  ViewLayer,
} from "@/ui";
import { useDeleteTask, useRunTaskNow, useTaskRunEvents, useTasks, useToggleTask } from "./hooks";
import { ReportView } from "./ReportView";
import { describeSchedule, describeScope } from "./schedule";
import { TaskDialog } from "./TaskDialog";

export function TasksView({ taskRunId }: { taskRunId?: string }) {
  useTaskRunEvents();
  if (taskRunId) return <ReportView runId={taskRunId} />;
  return <TaskList />;
}

/** How a task stands, for its row and the command palette. */
export function taskStatus(task: Task): { tone: StatusTone; label: string } {
  if (task.last_run?.status === "running") return { tone: "running", label: "Running" };
  if (!task.enabled) return { tone: "neutral", label: "Off" };
  if (task.last_run?.status === "failed") return { tone: "danger", label: "Last run failed" };
  if (task.last_run?.status === "succeeded") return { tone: "success", label: "Last run finished" };
  return { tone: "neutral", label: "Not run yet" };
}

function TaskList() {
  const reduced = useReducedMotion();
  const { data: tasks = [], isLoading } = useTasks();
  const { data: categories = [] } = useCategories();
  const toggle = useToggleTask();
  const run = useRunTaskNow();
  const remove = useDeleteTask();
  const [editing, setEditing] = useState<{ task: Task | null } | null>(null);
  const [confirm, setConfirm] = useState<Task | null>(null);

  return (
    <ViewLayer>
      <CanvasBand
        start={<Crumb parts={["Scheduled tasks"]} />}
        end={
          <Button
            size="sm"
            variant="secondary"
            icon={Plus}
            iconMotion="plus"
            onClick={() => setEditing({ task: null })}
          >
            New task
          </Button>
        }
      />
      <ScrollArea>
        <PageColumn>
          <header className="nt-page-head">
            <div>
              <h1 className="t-title">Scheduled tasks</h1>
              <p className="t-callout">
                Cilica works through your notes on a schedule and leaves suggestions for you to
                approve. Tasks never run while you are recording.
              </p>
            </div>
          </header>
          {isLoading ? (
            <SkeletonText lines={4} />
          ) : tasks.length === 0 ? (
            <EmptyState
              art="tasks"
              title="No scheduled tasks"
              tip={
                <ShortcutTip action="assistant.newThread">Or ask Cilica to set one up</ShortcutTip>
              }
              actions={
                <Button
                  variant="primary"
                  icon={Plus}
                  iconMotion="plus"
                  onClick={() => setEditing({ task: null })}
                >
                  New task
                </Button>
              }
            >
              Ask Cilica to tidy, summarise or review your notes every few days.
            </EmptyState>
          ) : (
            <div className="nt-list">
              <AnimatePresence initial={false}>
                {tasks.map((task) => {
                  const s = taskStatus(task);
                  const running = task.last_run?.status === "running";
                  const lastRun = task.last_run;
                  const openReport = () =>
                    lastRun && !running && navigate({ name: "tasks", taskRunId: lastRun.id });
                  return (
                    <motion.div
                      key={task.id}
                      className="nt-task-item"
                      layout="position"
                      initial={false}
                      exit={{ opacity: 0, x: reduced ? 0 : 8, transition: tweens.exit }}
                      transition={reduced ? reducedTransition : springs.smooth}
                    >
                      <ListItem
                        className="nt-task-row"
                        tabIndex={0}
                        onKeyDown={(e) => {
                          if (e.target === e.currentTarget && e.key === "Enter") openReport();
                        }}
                        heading={
                          <span className="nt-task-title">
                            <StatusDot tone={s.tone} label={s.label} />
                            <span className="nt-task-name">{task.name}</span>
                            {task.pending_count > 0 && lastRun && (
                              <button type="button" className="nt-badge-btn" onClick={openReport}>
                                <CountBadge>
                                  {task.pending_count === 1
                                    ? "1 proposal"
                                    : `${task.pending_count} proposals`}
                                </CountBadge>
                              </button>
                            )}
                          </span>
                        }
                        meta={
                          running ? (
                            <Shimmer>Running now…</Shimmer>
                          ) : (
                            [
                              describeSchedule(task.schedule),
                              describeScope(task.scope, categories),
                              task.enabled && task.next_run_at
                                ? `Next ${dayAndTime(task.next_run_at)}`
                                : "Off",
                              lastRun?.finished_at
                                ? `Last ran ${relativeDate(lastRun.finished_at)}`
                                : null,
                            ]
                              .filter(Boolean)
                              .join(" · ")
                          )
                        }
                        actions={
                          <>
                            {lastRun && !running && (
                              <Button size="sm" variant="ghost" onClick={openReport}>
                                View report
                              </Button>
                            )}
                            <Tooltip label="Run now" side="top">
                              <Button
                                size="sm"
                                variant="secondary"
                                icon={Play}
                                loading={run.isPending && run.variables === task.id}
                                disabled={running}
                                onClick={() => run.mutate(task.id)}
                                aria-label={`Run ${task.name} now`}
                              >
                                Run now
                              </Button>
                            </Tooltip>
                            <Menu>
                              <MenuTrigger asChild>
                                <IconButton icon={Ellipsis} label={`More for ${task.name}`} />
                              </MenuTrigger>
                              <MenuContent>
                                <MenuItem icon={Pencil} onSelect={() => setEditing({ task })}>
                                  Edit
                                </MenuItem>
                                <MenuItem
                                  icon={CalendarClock}
                                  iconMotion="clock"
                                  disabled={!lastRun || running}
                                  onSelect={openReport}
                                >
                                  Latest report
                                </MenuItem>
                                {!task.is_default && (
                                  <>
                                    <MenuSeparator />
                                    <MenuItem
                                      icon={Trash2}
                                      iconMotion="trash"
                                      danger
                                      onSelect={() => setConfirm(task)}
                                    >
                                      Delete
                                    </MenuItem>
                                  </>
                                )}
                              </MenuContent>
                            </Menu>
                          </>
                        }
                      />
                      <span className="nt-task-switch">
                        <Switch
                          label={`${task.name} on schedule`}
                          checked={task.enabled}
                          onCheckedChange={(enabled) => toggle.mutate({ task, enabled })}
                        />
                      </span>
                    </motion.div>
                  );
                })}
              </AnimatePresence>
            </div>
          )}
          {tasks.some((t) => t.is_default) && (
            <p className="nt-settings-foot">
              Notes review is built in. You can edit it or turn it off, but not delete it.
            </p>
          )}
        </PageColumn>
      </ScrollArea>
      <TaskDialog
        open={editing !== null}
        task={editing?.task ?? null}
        onOpenChange={(o) => !o && setEditing(null)}
      />
      <ConfirmDialog
        open={confirm !== null}
        onOpenChange={(o) => !o && setConfirm(null)}
        destructive
        title={`Delete ${confirm?.name ?? "this task"}?`}
        body="It stops running. Suggestions it already made stay in their reports until you answer them."
        confirmLabel="Delete task"
        onConfirm={() => confirm && remove.mutate(confirm.id)}
      />
    </ViewLayer>
  );
}
