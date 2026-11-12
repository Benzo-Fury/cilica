/**
 * Settings › Import: pick an app from a searchable list with icons; Margin says what it found on
 * this computer and exactly what to do, takes whatever that way needs (a folder, a file, a key or
 * pasted text), previews, imports in the background and keeps a history that can be undone.
 * Any app can be imported from any number of times; only new or changed notes come in again.
 */
import { AlertTriangle, ChevronDown, Square, Undo2 } from "lucide-react";
import { useMemo, useState } from "react";
import type { ImportProgress, ImportRun, ImportRunStatus } from "@/ipc/types";
import { relativeDate } from "@/lib/format";
import {
  Button,
  Chip,
  Collapse,
  ConfirmDialog,
  IconButton,
  IconPicker,
  type IconPickerGroup,
  Progress,
  ProviderIcon,
  SettingsGroup,
  SettingsPage,
} from "@/ui";
import {
  countNotes,
  providerName,
  useAllDetections,
  useCancelImport,
  useImportEvents,
  useImportProgress,
  useImportRuns,
  useUndoImport,
} from "./hooks";
import { ProviderPanel } from "./ProviderPanel";
import { GROUP_LABELS, groupedProviders, providerById } from "./providers";

export function ImportPage() {
  const [providerId, setProviderId] = useState<string | null>(null);
  const { data: found = [] } = useAllDetections();
  const provider = providerId ? providerById(providerId) : undefined;
  const groups: IconPickerGroup[] = useMemo(() => {
    const foundIds = new Map(found.map((d) => [d.provider, d]));
    return groupedProviders().map(({ group, providers }) => ({
      heading: GROUP_LABELS[group],
      options: providers.map((p) => {
        const d = foundIds.get(p.id);
        const badge =
          d && d.locations.length > 0
            ? "Found"
            : d?.has_key
              ? "Key saved"
              : d?.app_installed
                ? "Installed"
                : undefined;
        return {
          value: p.id,
          label: p.name,
          hint: p.tagline,
          keywords: p.keywords,
          icon: <ProviderIcon name={p.name} logo={p.logo} glyph={p.glyph} />,
          badge: badge ? (
            <span className={badge === "Found" ? "nt-picker-badge is-found" : "nt-picker-badge"}>
              {badge}
            </span>
          ) : undefined,
        };
      }),
    }));
  }, [found]);
  const suggestions = found.filter((d) => d.locations.length > 0).slice(0, 3);

  return (
    <SettingsPage
      title="Import"
      description="Bring notes, meetings and transcripts from other apps into Margin. Files and apps on this computer are read here; Margin only contacts another app's servers when you give it that app's API key."
    >
      <SettingsGroup title="Import from">
        <div className="nt-import-picker">
          <IconPicker
            label="App to import from"
            placeholder="Choose an app…"
            searchPlaceholder="Search 45 apps"
            value={providerId}
            groups={groups}
            onChange={setProviderId}
          />
          {!provider && suggestions.length > 0 && (
            <div className="nt-import-suggest">
              <span className="nt-faint">Found on this computer:</span>
              {suggestions.map((d) => {
                const p = providerById(d.provider);
                if (!p) return null;
                return (
                  <button
                    key={d.provider}
                    type="button"
                    className="nt-chip nt-anim nt-import-suggest-chip"
                    onClick={() => setProviderId(d.provider)}
                  >
                    <ProviderIcon name={p.name} logo={p.logo} glyph={p.glyph} size="sm" />
                    {p.name}
                  </button>
                );
              })}
            </div>
          )}
        </div>
      </SettingsGroup>

      {provider && (
        <div className="nt-card nt-import-card">
          <ProviderPanel key={provider.id} provider={provider} />
        </div>
      )}

      <ActiveImports />
      <PastImports />
    </SettingsPage>
  );
}

// ── Running ──────────────────────────────────────────────────────────────

function ActiveImports() {
  const runs = useImportProgress((s) => s.runs);
  const active = Object.values(runs);
  if (active.length === 0) return null;
  return (
    <SettingsGroup title="Importing">
      {active.map((p) => (
        <ProgressRow key={p.run_id} progress={p} />
      ))}
    </SettingsGroup>
  );
}

function ProgressRow({ progress }: { progress: ImportProgress }) {
  const cancel = useCancelImport();
  const forget = useImportProgress((s) => s.forget);
  const provider = providerById(progress.provider);
  const name = providerName(progress.provider);
  const done = progress.phase === "done";
  const value = done ? 1 : progress.total > 0 ? progress.done / progress.total : null;
  const run = progress.run;
  return (
    <div className="nt-settings-row nt-import-progress">
      <div className="nt-settings-row-main">
        <ProviderIcon name={name} logo={provider?.logo} glyph={provider?.glyph} />
        <div className="nt-settings-text">
          <div className="nt-settings-label">
            {done ? `Finished importing from ${name}` : `Importing from ${name}`}
          </div>
          <div className="nt-settings-desc" aria-live="polite">
            {done && run
              ? runSummary(run)
              : progress.phase === "waiting"
                ? `${progress.current ?? "Waiting"}. Margin carries on by itself.`
                : progress.total > 0
                  ? `${progress.done} of ${progress.total}${progress.current ? ` · ${progress.current}` : ""}`
                  : "Getting ready…"}
          </div>
        </div>
        {done ? (
          <Button size="sm" variant="ghost" onClick={() => forget(progress.run_id)}>
            Done
          </Button>
        ) : (
          <Button
            size="sm"
            variant="secondary"
            icon={Square}
            loading={cancel.isPending}
            onClick={() => cancel.mutate(progress.run_id)}
          >
            Stop
          </Button>
        )}
      </div>
      {!done && (
        <Progress value={value} label={`Importing from ${name}`} className="nt-import-bar" />
      )}
    </div>
  );
}

// ── History ──────────────────────────────────────────────────────────────

function PastImports() {
  const { data: runs = [] } = useImportRuns();
  const undo = useUndoImport();
  const [confirm, setConfirm] = useState<ImportRun | null>(null);
  const finished = runs.filter((r) => r.status !== "running");
  if (finished.length === 0) return null;
  return (
    <SettingsGroup
      title="Past imports"
      footer="Undo moves the notes an import added to Trash, where they stay for 30 days. Notes you have edited since go too."
    >
      {finished.map((run) => (
        <RunRow key={run.id} run={run} onUndo={() => setConfirm(run)} />
      ))}
      <ConfirmDialog
        open={confirm !== null}
        onOpenChange={(open) => !open && setConfirm(null)}
        title={confirm ? `Undo the import from ${providerName(confirm.provider)}?` : ""}
        body={
          confirm
            ? `${countNotes(confirm.imported)} it added will move to Trash. You can restore them from Trash for 30 days.`
            : ""
        }
        confirmLabel="Move to Trash"
        destructive
        onConfirm={() => confirm && undo.mutate(confirm.id)}
      />
    </SettingsGroup>
  );
}

const STATUS: Record<ImportRunStatus, { label: string; tone: "neutral" | "success" | "warning" }> =
  {
    running: { label: "Running", tone: "neutral" },
    succeeded: { label: "Done", tone: "success" },
    partial: { label: "Some skipped", tone: "warning" },
    failed: { label: "Stopped", tone: "warning" },
    cancelled: { label: "Stopped", tone: "neutral" },
    undone: { label: "Undone", tone: "neutral" },
  };

function RunRow({ run, onUndo }: { run: ImportRun; onUndo: () => void }) {
  const [open, setOpen] = useState(false);
  const provider = providerById(run.provider);
  const name = providerName(run.provider);
  const status = STATUS[run.status];
  return (
    <div className="nt-settings-row">
      <div className="nt-settings-row-main">
        <ProviderIcon name={name} logo={provider?.logo} glyph={provider?.glyph} />
        <div className="nt-settings-text">
          <div className="nt-settings-label nt-model-name">
            {name}
            <Chip tone={status.tone}>{status.label}</Chip>
          </div>
          <div className="nt-settings-desc">
            {run.origin} · {relativeDate(run.started_at)} · {runSummary(run)}
          </div>
        </div>
        {run.report.length > 0 && (
          <IconButton
            label={open ? "Hide details" : "Show details"}
            icon={ChevronDown}
            className={open ? "nt-import-chevron is-open" : "nt-import-chevron"}
            aria-expanded={open}
            onClick={() => setOpen(!open)}
          />
        )}
        {run.status !== "undone" && run.imported > 0 && (
          <Button size="sm" variant="ghost" icon={Undo2} iconMotion="undo" onClick={onUndo}>
            Undo
          </Button>
        )}
      </div>
      <Collapse open={open}>
        <ul className="nt-import-report">
          {run.report.map((line) => (
            <li key={line}>
              <AlertTriangle className="nt-ico nt-ico-sm" strokeWidth={1.75} aria-hidden />
              {line}
            </li>
          ))}
        </ul>
      </Collapse>
    </div>
  );
}

function runSummary(run: ImportRun): string {
  const parts = [
    run.imported > 0 ? `${run.imported} imported` : null,
    run.updated > 0 ? `${run.updated} updated` : null,
    run.skipped > 0 ? `${run.skipped} already here` : null,
    run.failed > 0 ? `${run.failed} could not be read` : null,
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : "Nothing new";
}

// ── App-wide ─────────────────────────────────────────────────────────────

/**
 * Says when an import finishes, wherever the person is, with Undo. Undo still asks first: it
 * moves every note the import added to Trash.
 */
export function ImportWatcher() {
  const undo = useUndoImport();
  const [confirm, setConfirm] = useState<ImportRun | null>(null);
  useImportEvents(setConfirm);
  return (
    <ConfirmDialog
      open={confirm !== null}
      onOpenChange={(open) => !open && setConfirm(null)}
      title={confirm ? `Undo the import from ${providerName(confirm.provider)}?` : ""}
      body={
        confirm
          ? `${countNotes(confirm.imported)} it added will move to Trash. You can restore them from Trash for 30 days.`
          : ""
      }
      confirmLabel="Move to Trash"
      destructive
      onConfirm={() => confirm && undo.mutate(confirm.id)}
    />
  );
}
