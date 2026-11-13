/**
 * Settings › Import: pick an app from a searchable list with icons; Margin says what it found on
 * this computer and exactly what to do, takes whatever that way needs (a folder, a file, a key or
 * pasted text), previews and imports in the background. The done toast can undo an import; no
 * history is kept. Any app can be imported from any number of times: items that are already notes
 * are skipped, and notes that were deleted or undone come back.
 */
import { Square } from "lucide-react";
import { useMemo, useState } from "react";
import type { ImportProgress, ImportRun } from "@/ipc/types";
import {
  Button,
  ConfirmDialog,
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

function runSummary(run: ImportRun): string {
  const parts = [
    run.imported > 0 ? `${run.imported} imported` : null,
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
