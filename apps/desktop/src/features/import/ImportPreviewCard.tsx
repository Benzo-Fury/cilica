/**
 * What an import will do, before it writes anything: counts, what comes across, where notes go,
 * which speaker is you, what is left out and a few sample notes.
 */
import {
  ArrowLeft,
  AudioLines,
  Check,
  CircleAlert,
  FileText,
  Minus,
  UserRound,
} from "lucide-react";
import { useMemo, useState } from "react";
import type { ImportCategoryMode, ImportFidelity, ImportPreview } from "@/ipc/types";
import { plural, relativeDate } from "@/lib/format";
import { Button, Chip, ChipButton, Icon, ProviderIcon, RichText, SelectMenu, StatGrid } from "@/ui";
import { countNotes, useStartImport } from "./hooks";
import type { ImportProvider } from "./providers";

const FIDELITY: { key: keyof ImportFidelity; label: string }[] = [
  { key: "summary", label: "Summary" },
  { key: "my_notes", label: "Notes" },
  { key: "transcript", label: "Transcript" },
  { key: "speakers", label: "Speakers" },
  { key: "timestamps", label: "Timestamps" },
  { key: "attachments", label: "Images" },
];

export function ImportPreviewCard({
  provider,
  preview,
  onBack,
  onStarted,
}: {
  provider: ImportProvider;
  preview: ImportPreview;
  onBack: () => void;
  onStarted: () => void;
}) {
  const start = useStartImport();
  const hasFolders = preview.folders.length > 0;
  const [mode, setMode] = useState<ImportCategoryMode>(
    hasFolders ? "keep_folders" : "one_category",
  );
  const [me, setMe] = useState<string[]>([]);
  const toImport = Math.max(0, preview.total - preview.unchanged);
  const fallback = `From ${provider.name}`;
  const modes = useMemo(
    () =>
      [
        hasFolders
          ? {
              value: "keep_folders" as const,
              label: "Keep folders as categories",
              hint: `Notes outside a folder go to “${fallback}”.`,
            }
          : null,
        { value: "one_category" as const, label: `One category, “${fallback}”` },
        { value: "none" as const, label: "No category" },
      ].filter((m) => m !== null),
    [hasFolders, fallback],
  );
  const dates =
    preview.earliest && preview.latest
      ? preview.earliest.slice(0, 10) === preview.latest.slice(0, 10)
        ? formatDay(preview.earliest)
        : `${formatDay(preview.earliest)} to ${formatDay(preview.latest)}`
      : null;

  return (
    <section
      className="nt-import-panel nt-import-preview"
      aria-label={`Preview of the import from ${provider.name}`}
    >
      <header className="nt-import-head">
        <ProviderIcon name={provider.name} logo={provider.logo} glyph={provider.glyph} size="lg" />
        <div className="nt-import-head-text">
          <h2 className="t-headline">Ready to import</h2>
          <p className="nt-faint">From {preview.origin}</p>
        </div>
        <Button size="sm" variant="ghost" icon={ArrowLeft} onClick={onBack}>
          Back
        </Button>
      </header>

      <StatGrid
        items={[
          { label: "Notes", value: preview.notes },
          { label: "Meetings", value: preview.meetings },
          {
            label: "Transcripts",
            value: preview.details_pending ? "With each meeting" : preview.transcripts,
          },
          { label: "Images", value: preview.attachments },
        ].filter((s) => s.value !== 0)}
      />
      {dates && <p className="nt-import-dates">{dates}</p>}

      <div>
        <div className="nt-import-subtitle">What comes across</div>
        <ul className="nt-import-carries">
          {FIDELITY.map(({ key, label }) => (
            <li key={key}>
              <Chip
                icon={preview.includes[key] ? Check : Minus}
                tone={preview.includes[key] ? "success" : "neutral"}
                className={preview.includes[key] ? undefined : "nt-chip-off"}
              >
                {label}
              </Chip>
            </li>
          ))}
        </ul>
        {preview.details_pending && (
          <p className="nt-faint nt-import-note">
            Margin reads each meeting's summary and transcript while it imports.
          </p>
        )}
      </div>

      {(preview.unchanged > 0 || preview.changed > 0) && (
        <p className="nt-import-callout" role="note">
          <Icon icon={Check} size="sm" />
          <span>
            {preview.unchanged > 0 &&
              `${countNotes(preview.unchanged)} imported before ${preview.details_pending ? "will be skipped" : "and unchanged will be skipped"}. `}
            {preview.changed > 0 &&
              `${countNotes(preview.changed)} changed since and will be updated, unless you edited them in Margin.`}
          </span>
        </p>
      )}

      <div className="nt-import-options">
        <div className="nt-import-option">
          <div>
            <div className="nt-settings-label">Where notes go</div>
            {hasFolders && (
              <div className="nt-settings-desc">
                {plural(preview.folders.length, "folder")}: {preview.folders.slice(0, 4).join(", ")}
                {preview.folders.length > 4 ? "…" : ""}
              </div>
            )}
          </div>
          <SelectMenu<ImportCategoryMode>
            label="Where notes go"
            value={mode}
            options={modes}
            onChange={setMode}
          />
        </div>
        {preview.speakers.length > 0 && (
          <div className="nt-import-option nt-import-option-col">
            <div>
              <div className="nt-settings-label">Which of these is you?</div>
              <div className="nt-settings-desc">
                Your lines show as “You”, as in meetings Margin records. Skip this if none is you.
              </div>
            </div>
            <fieldset className="nt-import-speakers" aria-label="Speakers">
              {preview.speakers.slice(0, 12).map((s) => {
                const on = me.includes(s.name);
                return (
                  <ChipButton
                    key={s.name}
                    tone={on ? "accent" : "neutral"}
                    icon={on ? Check : UserRound}
                    aria-pressed={on}
                    onClick={() => setMe(on ? me.filter((n) => n !== s.name) : [...me, s.name])}
                  >
                    {s.name}
                    <span className="nt-faint">{s.meetings}</span>
                  </ChipButton>
                );
              })}
            </fieldset>
          </div>
        )}
      </div>

      {preview.unsupported.filter((u) => u.count > 0).length > 0 && (
        <div>
          <div className="nt-import-subtitle">Not imported</div>
          <ul className="nt-import-leftout">
            {preview.unsupported
              .filter((u) => u.count > 0)
              .map((u) => (
                <li key={u.kind}>
                  <b>
                    {u.kind} ({u.count})
                  </b>
                  <span>
                    <RichText text={u.reason} />
                  </span>
                </li>
              ))}
          </ul>
        </div>
      )}

      {preview.warnings.length > 0 && (
        <ul className="nt-import-warnings">
          {preview.warnings.slice(0, 6).map((w) => (
            <li key={w}>
              <Icon icon={CircleAlert} size="sm" />
              {w}
            </li>
          ))}
        </ul>
      )}

      {preview.samples.length > 0 && (
        <div>
          <div className="nt-import-subtitle">Most recent</div>
          <ul className="nt-import-samples">
            {preview.samples.map((s) => (
              <li key={`${s.title}-${s.date ?? ""}`}>
                <Icon icon={s.is_meeting ? AudioLines : FileText} className="nt-faint" />
                <div className="nt-import-sample-text">
                  <div className="nt-import-sample-title">
                    <span>{s.title}</span>
                    {s.date && <span className="nt-faint">{relativeDate(s.date)}</span>}
                  </div>
                  {s.excerpt && <div className="nt-import-sample-excerpt">{s.excerpt}</div>}
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="nt-import-actions nt-import-actions-end">
        <Button variant="secondary" onClick={onBack}>
          Cancel
        </Button>
        <Button
          variant="primary"
          disabled={toImport === 0}
          loading={start.isPending}
          onClick={() =>
            start.mutate(
              { scanId: preview.scan_id, options: { category_mode: mode, self_names: me } },
              { onSuccess: onStarted },
            )
          }
        >
          {toImport === 0 ? "Nothing new to import" : `Import ${countNotes(toImport)}`}
        </Button>
      </div>
    </section>
  );
}

function formatDay(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}
