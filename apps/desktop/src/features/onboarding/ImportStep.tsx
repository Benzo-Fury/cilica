/**
 * Step 4: notes and meetings found in other apps on this computer. Picking some starts their
 * imports, which run in the background and report in pills and, when done, a toast with Undo.
 * Everything else (files, accounts, the other apps) stays in Settings › Import.
 */
import { useState } from "react";
import {
  countNotes,
  providerName,
  useAllDetections,
  useImportProgress,
  useImportScan,
  useStartImport,
} from "@/features/import/hooks";
import { PROVIDERS, providerById } from "@/features/import/providers";
import { errorMessage } from "@/ipc/client";
import type { ImportDetection, ImportPreview } from "@/ipc/types";
import { toast } from "@/lib/toast";
import { Button, SetupApp, SetupApps, SetupNote, SetupStep } from "@/ui";
import { formatCount, type StepProps, THIS_DEVICE } from "./shared";

/** How many tiles fit the window: two rows of three. */
const SHOWN = 6;

export interface FoundApp {
  provider: string;
  name: string;
  logo: string | undefined;
  /** Notes and meetings Cilica can read. */
  count: number;
  detail: string;
}

/** Apps with something to import, largest first. */
export function foundApps(detections: readonly ImportDetection[]): FoundApp[] {
  return detections
    .map((d): FoundApp | null => {
      const provider = providerById(d.provider);
      const notes = d.locations.reduce((n, l) => n + l.notes, 0);
      const meetings = d.locations.reduce((n, l) => n + l.meetings, 0);
      const count = notes + meetings;
      const detail =
        meetings > notes
          ? `${formatCount(meetings)} ${meetings === 1 ? "meeting" : "meetings"}`
          : `${formatCount(notes)} ${notes === 1 ? "note" : "notes"}`;
      return provider && count > 0
        ? { provider: d.provider, name: provider.name, logo: provider.logo, count, detail }
        : null;
    })
    .filter((a): a is FoundApp => a !== null)
    .sort((a, b) => b.count - a.count);
}

/** "Anarlog", "Anarlog and Bear", "3 apps". */
export function joinNames(names: readonly string[]): string {
  if (names.length <= 2) return names.join(" and ");
  return `${names.length} apps`;
}

export function ImportStep({ status, skipped, onNext, onReopen }: StepProps) {
  const { data: detections, isLoading } = useAllDetections();
  const found = foundApps(detections ?? []).slice(0, SHOWN);
  const [picked, setPicked] = useState<ReadonlySet<string> | null>(null);
  const selected = picked ?? new Set(found.slice(0, 1).map((a) => a.provider));
  const [runIds, setRunIds] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const scan = useImportScan();
  const start = useStartImport();
  const runs = useImportProgress((s) => s.runs);

  const mine = runIds.map((id) => runs[id]).filter((r) => r !== undefined);
  const total = mine.reduce((n, r) => n + r.total, 0);
  const done = mine.reduce((n, r) => n + r.done, 0);
  const names = joinNames([...new Set(mine.map((r) => providerName(r.provider)))]);
  const finished = mine.length > 0 && mine.every((r) => r.phase === "done");
  const count = found.filter((a) => selected.has(a.provider)).reduce((n, a) => n + a.count, 0);

  const summary =
    runIds.length === 0
      ? found.length === 0 && !isLoading && !skipped
        ? "No other apps found to import from"
        : "Skipped. Import any time from Settings › Import"
      : finished
        ? `Imported ${countNotes(done)} from ${names}`
        : `Importing from ${names} · ${formatCount(done)} of ${formatCount(total)}`;

  const toggle = (provider: string) => {
    const next = new Set(selected);
    if (next.has(provider)) next.delete(provider);
    else next.add(provider);
    setPicked(next);
  };

  const importSelected = async () => {
    setBusy(true);
    const started: string[] = [];
    for (const app of found.filter((a) => selected.has(a.provider))) {
      let preview: ImportPreview;
      try {
        preview = await scan.mutateAsync({ provider: app.provider, input: { type: "local" } });
      } catch (error) {
        toast.error(`Cilica could not read ${app.name}`, {
          description: `${errorMessage(error)} Try again from Settings › Import.`,
        });
        continue;
      }
      try {
        const run = await start.mutateAsync({
          scanId: preview.scan_id,
          options: {
            category_mode: preview.folders.length > 0 ? "keep_folders" : "one_category",
            self_names: [],
          },
        });
        started.push(run.id);
      } catch {
        // useStartImport has already said why.
      }
    }
    setBusy(false);
    setRunIds((ids) => [...ids, ...started]);
    if (started.length > 0) onNext(false);
  };

  // Apps only: "Anything else" (plain files) is not an app.
  const others = PROVIDERS.filter((p) => p.group !== "other").length - found.length;

  return (
    <SetupStep
      id="import"
      status={status}
      title="Bring your notes"
      summary={summary}
      skipped={skipped && runIds.length === 0}
      onReopen={onReopen}
      onSkip={() => onNext(runIds.length === 0)}
      description={
        isLoading
          ? `Looking for notes from other apps on ${THIS_DEVICE}…`
          : found.length > 0
            ? `Cilica found these on ${THIS_DEVICE}. Imports keep folders, dates and transcripts, and run in the background.`
            : `Cilica did not find notes from other apps on ${THIS_DEVICE}. Import files, or connect a meeting note taker, any time in Settings › Import.`
      }
      actions={
        found.length > 0 ? (
          <>
            <Button
              variant="primary"
              disabled={count === 0}
              loading={busy}
              onClick={() => void importSelected()}
            >
              {count === 0 ? "Choose what to import" : `Import ${countNotes(count)}`}
            </Button>
            <SetupNote>{others} more apps in Settings › Import.</SetupNote>
          </>
        ) : (
          !isLoading && (
            <Button variant="primary" onClick={() => onNext(false)}>
              Continue
            </Button>
          )
        )
      }
    >
      {found.length > 0 && (
        <SetupApps>
          {found.map((app) => (
            <SetupApp
              key={app.provider}
              logo={app.logo}
              name={app.name}
              detail={app.detail}
              selected={selected.has(app.provider)}
              disabled={busy}
              onToggle={() => toggle(app.provider)}
            />
          ))}
        </SetupApps>
      )}
    </SetupStep>
  );
}
