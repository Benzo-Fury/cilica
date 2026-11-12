/**
 * Pills for work that keeps going while you move on: speech and local model downloads, Ollama's
 * setup and imports. They leave when the work finishes; the finished toast says so.
 */

import { useQueryClient } from "@tanstack/react-query";
import { qk } from "@/app/queryKeys";
import { providerName, useImportProgress } from "@/features/import/hooks";
import {
  OLLAMA_APP,
  OLLAMA_PREFIX,
  SPEAKER_MODELS,
  useDownloads,
} from "@/features/settings/system";
import type { LocalLlmModel, SttModel } from "@/ipc/types";
import { SetupTask } from "@/ui";
import { formatCount, fraction } from "./shared";

export function BackgroundTasks() {
  const queryClient = useQueryClient();
  const downloads = useDownloads((s) => s.downloads);
  const runs = useImportProgress((s) => s.runs);
  const sttName = (id: string) =>
    queryClient.getQueryData<SttModel[]>(qk.sttModels)?.find((m) => m.id === id)?.name ??
    "the speech model";
  const localName = (tag: string) =>
    queryClient.getQueryData<LocalLlmModel[]>(qk.localModels)?.find((m) => m.id === tag)?.name ??
    tag;

  const tasks: { key: string; label: string; value: number | null }[] = [];
  for (const [id, download] of Object.entries(downloads)) {
    if (download.error || id === SPEAKER_MODELS) continue;
    const value = fraction(download);
    const pct = value === null ? "" : ` · ${Math.round(value * 100)}%`;
    const label =
      id === OLLAMA_APP
        ? `Setting up Ollama${pct}`
        : id.startsWith(OLLAMA_PREFIX)
          ? `Downloading ${localName(id.slice(OLLAMA_PREFIX.length))}${pct}`
          : `Downloading ${sttName(id)}${pct}`;
    tasks.push({ key: id, label, value });
  }
  for (const run of Object.values(runs)) {
    if (run.phase === "done") continue;
    const counts = run.total > 0 ? ` · ${formatCount(run.done)} of ${formatCount(run.total)}` : "";
    tasks.push({
      key: run.run_id,
      label: `Importing from ${providerName(run.provider)}${counts}`,
      value: run.total > 0 ? run.done / run.total : null,
    });
  }
  return (
    <>
      {tasks.map((t) => (
        <SetupTask key={t.key} label={t.label} value={t.value} />
      ))}
    </>
  );
}
