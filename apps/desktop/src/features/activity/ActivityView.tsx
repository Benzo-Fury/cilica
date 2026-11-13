/**
 * Cilica activity: the audit log of every tool call (spec › Guardrails). Each row has the tool's
 * past-tense summary, when, its tier and status, the arguments on demand, Undo while it can
 * still be undone, and the suggestion it made. Windowed, so hundreds of rows stay smooth.
 */

import { useVirtualizer } from "@tanstack/react-virtual";
import {
  Check,
  ChevronRight,
  CircleDashed,
  Clock,
  Lightbulb,
  type LucideIcon,
  TriangleAlert,
  Undo2,
  X,
} from "lucide-react";
import { memo, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { ShortcutTip } from "@/app/keybinds";
import { formatArgs } from "@/features/assistant/AssistantPanel";
import { ProposalCard } from "@/features/proposals/ProposalCard";
import { TIER_LABEL, toolIcon, toolName } from "@/features/proposals/tools";
import { CanvasBand } from "@/features/shell/CanvasBand";
import type { AiAction, AiActionStatus } from "@/ipc/types";
import { dayAndTime, relativeDate } from "@/lib/format";
import {
  Button,
  Chip,
  type ChipTone,
  Collapse,
  Crumb,
  DialogRoot,
  DialogSurface,
  DialogTitle,
  EmptyState,
  Icon,
  PageColumn,
  ScrollArea,
  SkeletonText,
  Spinner,
  ViewLayer,
} from "@/ui";
import { useAiActions, useUndoAction } from "./hooks";

/** Status is never colour alone: every chip carries an icon. */
const STATUS: Record<AiActionStatus, { label: string; tone: ChipTone; icon: LucideIcon }> = {
  running: { label: "Running", tone: "neutral", icon: CircleDashed },
  applied: { label: "Applied", tone: "success", icon: Check },
  proposed: { label: "Waiting for you", tone: "accent", icon: Clock },
  rejected: { label: "Rejected", tone: "neutral", icon: X },
  undone: { label: "Undone", tone: "neutral", icon: Undo2 },
  failed: { label: "Failed", tone: "warning", icon: TriangleAlert },
};

export function ActivityView() {
  const { data, isLoading, fetchNextPage, hasNextPage, isFetchingNextPage } = useAiActions();
  const actions = data?.pages.flat() ?? [];
  const scrollRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const [margin, setMargin] = useState(0);
  const [proposalId, setProposalId] = useState<string | null>(null);
  const virtualizer = useVirtualizer({
    count: actions.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 60,
    overscan: 10,
    scrollMargin: margin,
    getItemKey: (i) => actions[i]?.id ?? i,
  });
  useLayoutEffect(() => {
    if (listRef.current) setMargin(listRef.current.offsetTop);
  });

  // Load the next page as the end comes into view.
  const items = virtualizer.getVirtualItems();
  const lastIndex = items[items.length - 1]?.index ?? 0;
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage && lastIndex >= actions.length - 8) void fetchNextPage();
  }, [lastIndex, actions.length, hasNextPage, isFetchingNextPage, fetchNextPage]);

  return (
    <ViewLayer>
      <CanvasBand start={<Crumb parts={["Cilica activity"]} />} />
      <ScrollArea ref={scrollRef}>
        <PageColumn>
          <header className="nt-page-head">
            <div>
              <h1 className="t-title">Cilica activity</h1>
              <p>Everything Cilica read or changed, with an Undo while it still applies.</p>
            </div>
          </header>
          {isLoading ? (
            <SkeletonText lines={6} />
          ) : actions.length === 0 ? (
            <EmptyState
              art="activity"
              title="Cilica has not done anything yet"
              tip={<ShortcutTip action="assistant.toggle">Open Cilica from anywhere</ShortcutTip>}
            >
              When you ask Cilica to find, file or edit notes, every step it takes is listed here.
            </EmptyState>
          ) : (
            <ul ref={listRef} className="nt-virtual" style={{ height: virtualizer.getTotalSize() }}>
              {items.map((item) => {
                const action = actions[item.index];
                if (!action) return null;
                return (
                  <li
                    key={item.key}
                    data-index={item.index}
                    ref={virtualizer.measureElement}
                    style={{ transform: `translateY(${item.start - margin}px)` }}
                  >
                    <ActionRow action={action} onProposal={setProposalId} />
                  </li>
                );
              })}
            </ul>
          )}
          {isFetchingNextPage && (
            <div className="nt-list-more">
              <Spinner label="Loading more" />
            </div>
          )}
        </PageColumn>
      </ScrollArea>
      <DialogRoot open={proposalId !== null} onOpenChange={(o) => !o && setProposalId(null)}>
        {proposalId && (
          <DialogSurface className="nt-dialog-wide">
            <DialogTitle>Cilica's suggestion</DialogTitle>
            <ProposalCard proposalId={proposalId} />
          </DialogSurface>
        )}
      </DialogRoot>
    </ViewLayer>
  );
}

const ActionRow = memo(function ActionRow({
  action,
  onProposal,
}: {
  action: AiAction;
  onProposal: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const undo = useUndoAction();
  const id = useId();
  const status = STATUS[action.status];
  const details = formatArgs(action.args_json, action.error);
  return (
    <div className="nt-activity-row nt-anim-host" data-open={open || undefined}>
      <div className="nt-activity-main">
        <span className="nt-activity-icon">
          <Icon icon={toolIcon(action.tool)} size="sm" />
        </span>
        <button
          type="button"
          className="nt-activity-toggle"
          aria-expanded={open}
          aria-controls={id}
          onClick={() => setOpen((o) => !o)}
        >
          <span className="nt-activity-summary">{action.summary || toolName(action.tool)}</span>
          <span className="nt-activity-meta" title={dayAndTime(action.created_at)}>
            {toolName(action.tool)} · {relativeDate(action.created_at)}
          </span>
          <ChevronRight className="nt-ico nt-ico-sm nt-chevron" strokeWidth={1.75} aria-hidden />
        </button>
        <span className="nt-activity-tags">
          <Chip className="nt-tier-chip">{TIER_LABEL[action.tier]}</Chip>
          <Chip tone={status.tone} icon={status.icon}>
            {status.label}
          </Chip>
        </span>
        <span className="nt-activity-actions">
          {action.proposal_id && (
            <Button
              size="sm"
              variant="ghost"
              icon={Lightbulb}
              onClick={() => action.proposal_id && onProposal(action.proposal_id)}
            >
              Suggestion
            </Button>
          )}
          {action.can_undo && action.status === "applied" && (
            <Button
              size="sm"
              variant="secondary"
              icon={Undo2}
              iconMotion="undo"
              loading={undo.isPending}
              onClick={() => undo.mutate(action.id)}
            >
              Undo
            </Button>
          )}
        </span>
      </div>
      <Collapse open={open} id={id}>
        <pre className="nt-activity-args nt-selectable">{details ?? "No arguments."}</pre>
      </Collapse>
    </div>
  );
});
