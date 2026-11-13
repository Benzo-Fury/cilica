/**
 * A task run's report (spec › Default: Notes review): the summary, then suggestions grouped as
 * Merge, Trim, Delete and other, each on the same card as the assistant with Apply and Dismiss.
 * "Apply all" applies them one by one after a single confirm that names everything destructive.
 */
import { ArrowLeft, CheckCheck, CircleAlert } from "lucide-react";
import { useMemo, useState } from "react";
import { navigate } from "@/app/router";
import { useApplyProposal } from "@/features/proposals/hooks";
import { ProposalCard } from "@/features/proposals/ProposalCard";
import { CanvasBand } from "@/features/shell/CanvasBand";
import type { Proposal } from "@/ipc/types";
import { dayAndTime, displayTitle, plural } from "@/lib/format";
import { toast } from "@/lib/toast";
import {
  Button,
  Crumb,
  DialogActions,
  DialogDescription,
  DialogRoot,
  DialogSurface,
  DialogTitle,
  EmptyState,
  IconButton,
  PageColumn,
  ScrollArea,
  Shimmer,
  SkeletonText,
  ThreadNotice,
  ViewLayer,
} from "@/ui";
import { useTaskRun, useTasks } from "./hooks";

type Group = "merge" | "trim" | "delete" | "other";
const GROUPS: { id: Group; title: string; hint: string }[] = [
  { id: "merge", title: "Merge", hint: "Notes that cover the same topic." },
  { id: "trim", title: "Trim", hint: "Content that could go." },
  { id: "delete", title: "Delete", hint: "Notes, categories or people that look stale." },
  { id: "other", title: "Other suggestions", hint: "Renames, moves and new notes." },
];

export function groupOf(proposal: Proposal): Group {
  switch (proposal.kind.type) {
    case "merge_notes":
      return "merge";
    case "edit_note":
      return "trim";
    case "trash_notes":
    case "delete_category":
    case "delete_person":
      return "delete";
    default:
      return "other";
  }
}

/** One line naming what a destructive suggestion touches, for the Apply all confirm. */
export function destructiveLine(proposal: Proposal): string | null {
  if (proposal.tier !== "destructive") return null;
  const k = proposal.kind;
  const names = (notes: { title: string }[]) =>
    notes.map((n) => `“${displayTitle(n.title)}”`).join(", ");
  switch (k.type) {
    case "merge_notes":
      return `Merge ${names(k.sources)} into “${displayTitle(k.merged_title)}”; the originals move to Trash`;
    case "trash_notes":
      return `Move ${names(k.notes)} to Trash`;
    case "delete_category":
      return `Delete the category “${k.name}”; its ${plural(k.note_count, "note")} move to Notes`;
    case "delete_person":
      return `Delete ${k.name} from Saved people`;
    case "bulk_move":
      return `Move ${names(k.notes)} to ${k.category_name}`;
    case "move_groups":
      return k.groups.map((g) => `Move ${names(g.notes)} to ${g.category_name || "no category"}`).join("; ");
    default:
      return `Apply a change Cilica marked as destructive`;
  }
}

export function ReportView({ runId }: { runId: string }) {
  const { data, isLoading, isError } = useTaskRun(runId);
  const { data: tasks = [] } = useTasks();
  const apply = useApplyProposal({ undoToast: false });
  const [confirming, setConfirming] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const task = tasks.find((t) => t.id === data?.run.task_id);
  const proposals = data?.proposals ?? [];
  const pending = proposals.filter((p) => p.status === "pending");
  const destructive = pending.map(destructiveLine).filter((l): l is string => l !== null);
  const grouped = useMemo(() => {
    const map = new Map<Group, Proposal[]>();
    for (const p of proposals) map.set(groupOf(p), [...(map.get(groupOf(p)) ?? []), p]);
    return map;
  }, [proposals]);

  const applyAll = async () => {
    setConfirming(false);
    const queue = [...pending];
    let applied = 0;
    let failed = 0;
    setProgress({ done: 0, total: queue.length });
    for (const p of queue) {
      try {
        await apply.mutateAsync({ proposalId: p.id, finalMd: null });
        applied += 1;
      } catch {
        failed += 1;
      }
      setProgress({ done: applied + failed, total: queue.length });
    }
    setProgress(null);
    if (failed === 0) toast.success(`Applied ${plural(applied, "suggestion")}`);
    else
      toast.error(`Applied ${applied} of ${queue.length} suggestions`, {
        description: `${plural(failed, "suggestion")} could not be applied. They stay open below; ask Cilica to run the task again for fresh ones.`,
      });
  };

  const when = data ? dayAndTime(data.run.finished_at ?? data.run.started_at) : "";
  return (
    <ViewLayer>
      <CanvasBand
        back={false}
        start={
          <>
            <IconButton
              icon={ArrowLeft}
              iconMotion="arrow"
              label="Back to Scheduled tasks"
              onClick={() => navigate({ name: "tasks" })}
            />
            <Crumb
              parts={["Scheduled tasks", `${task?.name ?? "Report"}${when ? ` · ${when}` : ""}`]}
            />
          </>
        }
        end={
          pending.length > 1 ? (
            <Button
              size="sm"
              variant="primary"
              icon={CheckCheck}
              loading={progress !== null}
              onClick={() => (destructive.length ? setConfirming(true) : void applyAll())}
            >
              Apply all {pending.length}
            </Button>
          ) : undefined
        }
      />
      <ScrollArea>
        <PageColumn>
          {isLoading ? (
            <SkeletonText lines={6} />
          ) : isError || !data ? (
            <EmptyState art="tasks" title="This report is not available">
              The run may have been removed. Go back to Scheduled tasks and open another one.
            </EmptyState>
          ) : (
            <>
              <header className="nt-page-head nt-report-head">
                <div>
                  <h1 className="t-title">{task?.name ?? "Report"}</h1>
                  <p className="t-callout">
                    {data.run.status === "running" ? (
                      <Shimmer>Running now…</Shimmer>
                    ) : (
                      data.run.summary || "Cilica found nothing to suggest."
                    )}
                  </p>
                  {progress && (
                    <p className="t-callout nt-faint" aria-live="polite">
                      Applying{" "}
                      {progress.done + 1 > progress.total ? progress.total : progress.done + 1} of{" "}
                      {progress.total}…
                    </p>
                  )}
                </div>
              </header>
              {data.run.status === "failed" && (
                <ThreadNotice icon={CircleAlert} tone="danger">
                  {data.run.error ?? "The run failed."} Run the task again from Scheduled tasks.
                </ThreadNotice>
              )}
              {data.run.status !== "running" &&
                proposals.length === 0 &&
                data.run.status !== "failed" && (
                  <EmptyState art="tasks" title="Nothing to suggest">
                    Your notes look tidy. Cilica checks again on the next run.
                  </EmptyState>
                )}
              {GROUPS.map((group) => {
                const items = grouped.get(group.id);
                if (!items?.length) return null;
                const open = items.filter((p) => p.status === "pending").length;
                return (
                  <section key={group.id} className="nt-report-group">
                    <h2 className="nt-settings-group-title">
                      {group.title}
                      <span className="nt-report-count">{open ? `${open} open` : "Done"}</span>
                    </h2>
                    <p className="nt-report-hint">{group.hint}</p>
                    <div className="nt-report-cards">
                      {items.map((p) => (
                        <ProposalCard key={p.id} proposalId={p.id} initial={p} verbs="task" />
                      ))}
                    </div>
                  </section>
                );
              })}
            </>
          )}
        </PageColumn>
      </ScrollArea>
      <DialogRoot open={confirming} onOpenChange={setConfirming}>
        <DialogSurface className="nt-dialog-wide">
          <DialogTitle>Apply all {pending.length} suggestions?</DialogTitle>
          <DialogDescription asChild>
            <div className="nt-confirm-body">
              <p>
                These change or remove notes. Everything is versioned and trashed notes stay in
                Trash for 30 days.
              </p>
              <ul className="nt-confirm-list">
                {destructive.map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
            </div>
          </DialogDescription>
          <DialogActions>
            <Button variant="secondary" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button
              variant="danger"
              className="nt-btn-danger-solid"
              onClick={() => void applyAll()}
            >
              Apply all {pending.length}
            </Button>
          </DialogActions>
        </DialogSurface>
      </DialogRoot>
    </ViewLayer>
  );
}
