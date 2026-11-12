/**
 * One app in Settings › Import: what Margin found on this computer, each way to bring notes across
 * with its steps, the inputs that way needs, and what comes across or stays behind.
 */
import {
  Check,
  CircleAlert,
  ClipboardPaste,
  Database,
  FileUp,
  FolderOpen,
  Info,
  KeyRound,
  Monitor,
  RefreshCw,
} from "lucide-react";
import { useCallback, useId, useState } from "react";
import { toAppError } from "@/ipc/client";
import type {
  DetectedLocation,
  ImportDetection,
  ImportInput,
  ImportPreview,
  UnsupportedItem,
} from "@/ipc/types";
import { plural } from "@/lib/format";
import {
  Button,
  Chip,
  DropZone,
  Field,
  Icon,
  RichText,
  SegmentedControl,
  Spinner,
  StepList,
  TextArea,
} from "@/ui";
import { pickPaths, useFileDrop, useImportDetection, useImportKey, useImportScan } from "./hooks";
import { ImportPreviewCard } from "./ImportPreviewCard";
import { type LeftOutItem, LeftOutPills } from "./LeftOutPills";
import type { ImportProvider, ImportRoute } from "./providers";

export function ProviderPanel({ provider }: { provider: ImportProvider }) {
  const { data: detection, isFetching, refetch } = useImportDetection(provider.id);
  const scan = useImportScan();
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const routes = provider.routes.filter((r) => r.kind !== "local" || r.askFirst);
  const [routeIndex, setRouteIndex] = useState(0);
  const route = routes[routeIndex] ?? routes[0];
  const locations = detection?.locations ?? [];
  const hasLocalRoute = provider.routes.some((r) => r.kind === "local" && !r.askFirst);

  const run = useCallback(
    (input: ImportInput) => {
      setError(null);
      setPreview(null);
      scan.mutate(
        { provider: provider.id, input },
        { onSuccess: setPreview, onError: (e) => setError(toAppError(e).message) },
      );
    },
    [provider.id, scan.mutate],
  );

  if (preview) {
    return (
      <ImportPreviewCard
        provider={provider}
        preview={preview}
        onBack={() => setPreview(null)}
        onStarted={() => {
          setPreview(null);
          void refetch();
        }}
      />
    );
  }

  return (
    <section className="nt-import-panel" aria-label={provider.name}>
      {hasLocalRoute && (
        <LocalSection
          provider={provider}
          locations={locations}
          loading={isFetching && !detection}
          busy={scan.isPending}
          onRefresh={() => void refetch()}
          onImport={(path) => run({ type: "local", path })}
        />
      )}

      {detection?.note && (
        <p className="nt-import-callout" role="note">
          <Icon icon={Info} size="sm" />
          <span>{detection.note}</span>
        </p>
      )}

      {route && (
        <div className="nt-import-route">
          {routes.length > 1 && (
            <SegmentedControl
              label="How to import"
              value={String(routeIndex)}
              options={routes.map((r, i) => ({
                value: String(i),
                label: r.label,
                icon: ROUTE_ICONS[r.kind],
              }))}
              onChange={(v) => {
                setRouteIndex(Number(v));
                setError(null);
              }}
            />
          )}
          <RouteBody
            key={`${provider.id}-${routeIndex}`}
            provider={provider}
            route={route}
            detection={detection}
            busy={scan.isPending}
            onScan={run}
          />
        </div>
      )}

      {scan.isPending && (
        <div className="nt-import-status" role="status">
          <Spinner label="Reading" />
          <span>Reading {provider.name} notes…</span>
        </div>
      )}
      {error && (
        <p className="nt-key-bad nt-import-error" role="alert">
          <Icon icon={CircleAlert} size="sm" />
          {error}
        </p>
      )}

      <WhatComesAcross provider={provider} found={locations.flatMap((l) => l.unsupported)} />
    </section>
  );
}

const ROUTE_ICONS = {
  local: Monitor,
  files: FileUp,
  key: KeyRound,
  paste: ClipboardPaste,
} as const;

// ── Found on this computer ───────────────────────────────────────────────

function LocalSection({
  provider,
  locations,
  loading,
  busy,
  onRefresh,
  onImport,
}: {
  provider: ImportProvider;
  locations: DetectedLocation[];
  loading: boolean;
  busy: boolean;
  onRefresh: () => void;
  onImport: (path: string) => void;
}) {
  if (loading) {
    return (
      <div className="nt-import-status" role="status">
        <Spinner label="Looking" />
        <span>Looking for {provider.name} on this computer…</span>
      </div>
    );
  }
  if (locations.length === 0) {
    return (
      <div className="nt-import-none">
        <span>
          Margin did not find {provider.name} data on this computer. Use one of the ways below
          instead.
        </span>
        <Button size="sm" variant="ghost" icon={RefreshCw} onClick={onRefresh}>
          Look again
        </Button>
      </div>
    );
  }
  return (
    <div className="nt-import-found">
      <div className="nt-import-found-title">
        <Icon icon={Check} size="sm" />
        {locations.length === 1
          ? `Margin found ${provider.name} on this computer`
          : `Margin found ${locations.length} places on this computer`}
      </div>
      <ul className="nt-import-locations">
        {locations.map((location) => (
          <li key={location.path} className="nt-import-location">
            <Icon
              icon={
                location.path.endsWith(".db") || location.path.endsWith(".sqlite")
                  ? Database
                  : FolderOpen
              }
              className="nt-faint"
            />
            <div className="nt-import-location-text">
              <div className="nt-import-location-label">{location.label}</div>
              <div className="nt-import-counts">{countsLine(location)}</div>
              <div className="nt-import-path" title={location.path}>
                {location.path}
              </div>
            </div>
            <Button
              size="sm"
              variant="primary"
              loading={busy}
              onClick={() => onImport(location.path)}
            >
              Preview import
            </Button>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** "212 meetings, 180 with transcripts · 4 images" */
export function countsLine(c: {
  notes: number;
  meetings: number;
  transcripts: number;
  attachments: number;
}): string {
  const parts: string[] = [];
  if (c.notes > 0) parts.push(plural(c.notes, "note"));
  if (c.meetings > 0)
    parts.push(
      `${plural(c.meetings, "meeting")}${c.transcripts > 0 ? `, ${c.transcripts} with transcripts` : ""}`,
    );
  if (c.attachments > 0) parts.push(plural(c.attachments, "image"));
  return parts.length > 0 ? parts.join(" · ") : "Nothing to import yet";
}

// ── Each way in ──────────────────────────────────────────────────────────

function RouteBody({
  provider,
  route,
  detection,
  busy,
  onScan,
}: {
  provider: ImportProvider;
  route: ImportRoute;
  detection?: ImportDetection;
  busy: boolean;
  onScan: (input: ImportInput) => void;
}) {
  switch (route.kind) {
    case "local":
      return (
        <div className="nt-import-body">
          {route.steps && <StepList steps={route.steps} />}
          <div className="nt-import-actions">
            <Button
              variant="primary"
              icon={Monitor}
              loading={busy}
              onClick={() => onScan({ type: "local", path: null })}
            >
              {route.button ?? `Read ${provider.name}`}
            </Button>
          </div>
        </div>
      );
    case "files":
      return <FilesRoute provider={provider} route={route} busy={busy} onScan={onScan} />;
    case "key":
      return (
        <KeyRoute
          provider={provider}
          route={route}
          hasKey={detection?.has_key ?? false}
          busy={busy}
          onScan={onScan}
        />
      );
    case "paste":
      return <PasteRoute route={route} busy={busy} onScan={onScan} />;
  }
}

function FilesRoute({
  provider,
  route,
  busy,
  onScan,
}: {
  provider: ImportProvider;
  route: Extract<ImportRoute, { kind: "files" }>;
  busy: boolean;
  onScan: (input: ImportInput) => void;
}) {
  const onDrop = useCallback((paths: string[]) => onScan({ type: "paths", paths }), [onScan]);
  const over = useFileDrop(!busy, onDrop);
  const choose = async (kind: "files" | "folder") => {
    const paths = await pickPaths(
      kind,
      route.accept,
      kind === "folder"
        ? `Choose your ${provider.name} folder`
        : `Choose your ${provider.name} export`,
    );
    if (paths && paths.length > 0) onScan({ type: "paths", paths });
  };
  return (
    <div className="nt-import-body">
      <StepList steps={route.steps} />
      <DropZone
        active={over}
        title={route.folder ? "Drop files, a folder or a zip here" : "Drop the files here"}
        hint={<span>Margin reads {route.accept.map((e) => `.${e}`).join(", ")}</span>}
      >
        <Button
          size="sm"
          variant="secondary"
          icon={FileUp}
          disabled={busy}
          onClick={() => void choose("files")}
        >
          Choose files…
        </Button>
        {route.folder && (
          <Button
            size="sm"
            variant="secondary"
            icon={FolderOpen}
            disabled={busy}
            onClick={() => void choose("folder")}
          >
            Choose folder…
          </Button>
        )}
      </DropZone>
    </div>
  );
}

function KeyRoute({
  provider,
  route,
  hasKey,
  busy,
  onScan,
}: {
  provider: ImportProvider;
  route: Extract<ImportRoute, { kind: "key" }>;
  hasKey: boolean;
  busy: boolean;
  onScan: (input: ImportInput) => void;
}) {
  const id = useId();
  const { save, clear } = useImportKey(provider.id);
  const [key, setKey] = useState("");
  const [result, setResult] = useState<{ ok: boolean; message: string } | null>(null);
  const submit = () => {
    const trimmed = key.trim();
    if (!trimmed) return;
    setResult(null);
    save.mutate(trimmed, {
      onSuccess: (status) => {
        if (status.ok) {
          setKey("");
          setResult({
            ok: true,
            message: `${status.account ? `${status.account}. ` : ""}The key works and is saved in your keychain.`,
          });
        } else {
          setResult({
            ok: false,
            message:
              status.problem ??
              `${provider.name} didn't accept that key. Copy it again and paste it here.`,
          });
        }
      },
      onError: (e) => setResult({ ok: false, message: toAppError(e).message }),
    });
  };
  return (
    <div className="nt-import-body">
      <StepList steps={route.steps} />
      {route.plan && (
        <p className="nt-import-plan">
          <Icon icon={Info} size="sm" />
          <RichText text={route.plan} />
        </p>
      )}
      <div className="nt-import-key">
        <label htmlFor={id} className="nt-settings-label">
          {provider.name} API key
        </label>
        <span className="nt-key-control">
          <Field
            id={id}
            size="sm"
            type="password"
            icon={KeyRound}
            className="nt-setting-field"
            value={key}
            placeholder={hasKey ? "Saved in keychain" : (route.placeholder ?? "Paste your key")}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setKey(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                submit();
              }
            }}
          />
          <Button
            size="sm"
            variant={hasKey ? "secondary" : "primary"}
            disabled={!key.trim()}
            loading={save.isPending}
            onClick={submit}
          >
            {hasKey ? "Replace key" : "Save and check"}
          </Button>
          {hasKey && !key && (
            <Button
              size="sm"
              variant="ghost"
              loading={clear.isPending}
              onClick={() => clear.mutate()}
            >
              Remove
            </Button>
          )}
        </span>
        {result && (
          <span className={result.ok ? "nt-key-ok" : "nt-key-bad"} role="status">
            <Icon icon={result.ok ? Check : CircleAlert} size="sm" />
            {result.message}
          </span>
        )}
      </div>
      {hasKey && (
        <div className="nt-import-actions">
          <Button
            variant="primary"
            icon={RefreshCw}
            loading={busy}
            onClick={() => onScan({ type: "account" })}
          >
            Check my {provider.name} meetings
          </Button>
          <span className="nt-faint">
            Margin lists what's there first. Nothing is imported until you confirm.
          </span>
        </div>
      )}
    </div>
  );
}

function PasteRoute({
  route,
  busy,
  onScan,
}: {
  route: Extract<ImportRoute, { kind: "paste" }>;
  busy: boolean;
  onScan: (input: ImportInput) => void;
}) {
  const titleId = useId();
  const textId = useId();
  const [title, setTitle] = useState("");
  const [text, setText] = useState("");
  return (
    <div className="nt-import-body">
      <StepList steps={route.steps} />
      <div className="nt-import-paste">
        <label htmlFor={titleId} className="nt-settings-label">
          Title
        </label>
        <Field
          id={titleId}
          size="sm"
          value={title}
          placeholder="Weekly sync, 4 March"
          onChange={(e) => setTitle(e.target.value)}
        />
        <label htmlFor={textId} className="nt-settings-label">
          Notes or transcript
        </label>
        <TextArea
          id={textId}
          rows={8}
          value={text}
          placeholder={"Sam Lee  0:00\nThanks for joining…\n\nAna Ruiz  0:07\nHappy to be here."}
          onChange={(e) => setText(e.target.value)}
        />
      </div>
      <div className="nt-import-actions">
        <Button
          variant="primary"
          icon={ClipboardPaste}
          disabled={!text.trim()}
          loading={busy}
          onClick={() => onScan({ type: "paste", text, title: title.trim() || null })}
        >
          Preview
        </Button>
      </div>
    </div>
  );
}

// ── What comes across ─────────────────────────────────────────────────────

function WhatComesAcross({
  provider,
  found,
}: {
  provider: ImportProvider;
  found: UnsupportedItem[];
}) {
  const leftOut = mergeLeftOut(provider.leftOut ?? [], found);
  return (
    <div className="nt-import-fidelity">
      <div>
        <div className="nt-import-subtitle">What comes across</div>
        <ul className="nt-import-carries">
          {provider.carries.map((c) => (
            <li key={c}>
              <Chip icon={Check}>{c}</Chip>
            </li>
          ))}
        </ul>
      </div>
      {leftOut.length > 0 && <LeftOutPills items={leftOut} />}
    </div>
  );
}

/** Static notes from the catalogue, with counts where Margin found that kind of thing. */
function mergeLeftOut(
  fixed: { kind: string; reason: string }[],
  found: UnsupportedItem[],
): LeftOutItem[] {
  const out: LeftOutItem[] = fixed.map((f) => ({ ...f }));
  for (const item of found) {
    const match = out.find((o) => o.kind.toLowerCase() === item.kind.toLowerCase());
    if (match) match.count = (match.count ?? 0) + item.count;
    else if (item.count > 0) out.push({ kind: item.kind, reason: item.reason, count: item.count });
  }
  return out;
}
