/**
 * Step 2: where speech is turned into text. On this Mac, any of the Whisper models (the one that
 * suits this computer is marked; Download fetches it and switches to it, and the download keeps
 * going in a pill after setup moves on). Or a cloud service with the person's own key: the
 * engine only changes once the key is saved, so nothing is sent before then.
 */
import { useQueryClient } from "@tanstack/react-query";
import { Check, CircleAlert, Download, Globe, KeyRound, Type, X } from "lucide-react";
import { useId, useState } from "react";
import { qk } from "@/app/queryKeys";
import { useSettings, useUpdateSettings } from "@/features/settings/hooks";
import { CLOUD_PROVIDERS, type CloudEngine, providerName } from "@/features/settings/speech";
import {
  type Download as DownloadState,
  formatBytes,
  useDownloads,
  useHardwareProfile,
  useModelActions,
  useSttModels,
  useSttProviderKey,
  useSttProviderStatus,
} from "@/features/settings/system";
import { api, toAppError } from "@/ipc/client";
import type { Settings, SttModel } from "@/ipc/types";
import {
  Button,
  Chip,
  Field,
  Icon,
  IconButton,
  Progress,
  SetupCard,
  SetupChoice,
  SetupChoices,
  SetupField,
  SetupFieldRow,
  SetupFields,
  SetupList,
  SetupNote,
  SetupStep,
} from "@/ui";
import { fraction, type StepProps, THIS_DEVICE } from "./shared";

type Where = "local" | "cloud";
type Result = { ok: boolean; message: string } | null;

/** The recommended model first, the rest smallest first (the catalogue's order). */
export function orderModels(models: readonly SttModel[]): SttModel[] {
  return [...models.filter((m) => m.recommended), ...models.filter((m) => !m.recommended)];
}

export function SpeechStep({ status, skipped, onNext, onReopen }: StepProps) {
  const { data: settings } = useSettings();
  const { data: models = [] } = useSttModels();
  const { data: hardware } = useHardwareProfile();
  const downloads = useDownloads((s) => s.downloads);
  const t = settings?.transcription;
  const cloudEngine = t && t.engine !== "whisper_local" ? (t.engine as CloudEngine) : null;
  const { data: cloudStatus } = useSttProviderStatus(t?.engine ?? "whisper_local");
  const [where, setWhere] = useState<Where>(cloudEngine ? "cloud" : "local");

  const active = !cloudEngine ? models.find((m) => m.id === t?.model_id) : undefined;
  const busy = models.find((m) => downloads[m.id] && !downloads[m.id]?.error);
  const localReady = Boolean(active?.downloaded);
  const cloudReady = Boolean(cloudEngine && cloudStatus?.has_key);
  const configured = localReady || cloudReady || Boolean(busy);

  const summary = cloudReady
    ? `${providerName(cloudEngine as CloudEngine)} · key in your keychain`
    : busy
      ? `${busy.name} downloading · ${Math.round((fraction(downloads[busy.id]) ?? 0) * 100)}%`
      : localReady && active
        ? `${active.name} on ${THIS_DEVICE}`
        : "Skipped. Choose a model in Settings › Transcription";

  return (
    <SetupStep
      id="speech"
      status={status}
      title="Transcribe meetings"
      summary={summary}
      skipped={skipped && !configured}
      onReopen={onReopen}
      onSkip={() => onNext(!configured)}
      description={`A Whisper model on ${THIS_DEVICE} keeps audio private and free. A cloud service gives better live text on any computer, with your own key.`}
      actions={
        <Button variant={configured ? "primary" : "secondary"} onClick={() => onNext(!configured)}>
          {busy && !localReady && !cloudReady
            ? "Continue while it downloads"
            : configured
              ? "Continue"
              : "Skip for now"}
        </Button>
      }
    >
      <SetupChoices label="Where speech is transcribed">
        <SetupChoice
          title={`On ${THIS_DEVICE}`}
          badge={<Chip>Private</Chip>}
          description="Free. Audio never leaves it."
          checked={where === "local"}
          onSelect={() => setWhere("local")}
        />
        <SetupChoice
          title="Cloud service"
          description="Better live text. You pay the service."
          checked={where === "cloud"}
          onSelect={() => setWhere("cloud")}
        />
      </SetupChoices>
      {where === "local" ? (
        <>
          <SetupList>
            {orderModels(models).map((model) => (
              <ModelCard
                key={model.id}
                model={model}
                inUse={model.id === active?.id}
                download={downloads[model.id]}
              />
            ))}
          </SetupList>
          {hardware?.suggest_cloud && (
            <SetupNote>
              Live transcripts may lag on {THIS_DEVICE}. A cloud service gives better results for a
              few dollars a month.
            </SetupNote>
          )}
        </>
      ) : (
        settings && <CloudSetup settings={settings} active={cloudEngine} />
      )}
    </SetupStep>
  );
}

function ModelCard({
  model,
  inUse,
  download,
}: {
  model: SttModel;
  inUse: boolean;
  download: DownloadState | undefined;
}) {
  const { patch } = useUpdateSettings();
  const { downloadAndUse, cancel } = useModelActions();
  const downloading = download && !download.error;
  return (
    <SetupCard
      title={model.name}
      badge={model.recommended && <Chip tone="accent">Recommended for {THIS_DEVICE}</Chip>}
      description={`${model.description} ${formatBytes(model.size_mb * 1024 * 1024)}.`}
      progress={
        downloading && <Progress value={fraction(download)} label={`Downloading ${model.name}`} />
      }
      action={
        downloading ? (
          <IconButton
            icon={X}
            iconMotion="x"
            label={`Cancel the ${model.name} download`}
            onClick={() => cancel.mutate(model.id)}
          />
        ) : inUse && model.downloaded ? (
          <Chip tone="success" icon={Check}>
            In use
          </Chip>
        ) : model.downloaded ? (
          <Button
            variant="secondary"
            onClick={() => patch("transcription", { engine: "whisper_local", model_id: model.id })}
          >
            Use it
          </Button>
        ) : (
          <Button
            variant={model.recommended ? "primary" : "secondary"}
            icon={Download}
            iconMotion="up"
            onClick={() => downloadAndUse.mutate(model.id)}
          >
            {download?.error ? "Try again" : "Download"}
          </Button>
        )
      }
    />
  );
}

function CloudSetup({ settings, active }: { settings: Settings; active: CloudEngine | null }) {
  const [service, setService] = useState<CloudEngine>(active ?? "soniox");
  const provider = CLOUD_PROVIDERS.find((p) => p.value === service);
  return (
    <>
      <SetupChoices label="Transcription service" columns={3}>
        {CLOUD_PROVIDERS.map((p) => (
          <SetupChoice
            key={p.value}
            title={p.value === "open_ai_compatible" ? "Other" : p.label}
            description={p.price}
            checked={service === p.value}
            onSelect={() => setService(p.value)}
          />
        ))}
      </SetupChoices>
      <ServiceKey
        key={service}
        settings={settings}
        engine={service}
        inUse={active === service}
        hint={provider?.hint ?? ""}
      />
    </>
  );
}

function ServiceKey({
  settings,
  engine,
  inUse,
  hint,
}: {
  settings: Settings;
  engine: CloudEngine;
  inUse: boolean;
  hint: string;
}) {
  const urlId = useId();
  const modelId = useId();
  const keyId = useId();
  const queryClient = useQueryClient();
  const { data: status } = useSttProviderStatus(engine);
  const { save } = useSttProviderKey(engine);
  const { patch } = useUpdateSettings();
  const t = settings.transcription;
  const compatible = engine === "open_ai_compatible";
  const [url, setUrl] = useState(t.cloud.base_url ?? "");
  const [model, setModel] = useState(t.cloud.model ?? "");
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<Result>(null);
  const name = compatible ? "your service" : providerName(engine);
  const filled = Boolean(key.trim() && (!compatible || (url.trim() && model.trim())));
  const canUse = Boolean(status?.has_key) && !inUse && (!compatible || Boolean(t.cloud.base_url));

  const use = () => patch("transcription", { engine });

  const connect = async () => {
    if (!filled || busy) return;
    setBusy(true);
    setResult(null);
    try {
      if (compatible) {
        // The key check reads the address from Settings, so save it first.
        const saved = await api.updateSettings({
          ...settings,
          transcription: {
            ...t,
            cloud: { base_url: url.trim(), model: model.trim() },
          },
        });
        queryClient.setQueryData(qk.settings, saved);
      }
      const checked = await save.mutateAsync(key.trim());
      setKey("");
      use();
      setResult(
        checked.problem
          ? { ok: false, message: checked.problem }
          : {
              ok: true,
              message: `The key works. Meetings will be transcribed by ${name}.`,
            },
      );
    } catch (error) {
      setResult({ ok: false, message: toAppError(error).message });
    } finally {
      setBusy(false);
    }
  };

  return (
    <SetupFields>
      {compatible && (
        <>
          <SetupField label="Base URL" htmlFor={urlId}>
            <Field
              id={urlId}
              type="url"
              icon={Globe}
              value={url}
              placeholder="https://api.groq.com/openai/v1"
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setUrl(e.target.value)}
            />
          </SetupField>
          <SetupField label="Model" htmlFor={modelId}>
            <Field
              id={modelId}
              icon={Type}
              value={model}
              placeholder="whisper-large-v3-turbo"
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setModel(e.target.value)}
            />
          </SetupField>
        </>
      )}
      <SetupField label="API key" htmlFor={keyId}>
        <SetupFieldRow>
          <Field
            id={keyId}
            type="password"
            icon={KeyRound}
            value={key}
            placeholder={status?.has_key ? "Saved in your keychain" : "Paste your key"}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setKey(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void connect();
              }
            }}
          />
          {canUse && !key.trim() ? (
            <Button variant="primary" onClick={use}>
              Use {compatible ? "this service" : name}
            </Button>
          ) : (
            <Button
              variant="primary"
              disabled={!filled}
              loading={busy}
              onClick={() => void connect()}
            >
              {status?.has_key ? "Replace key" : "Save key"}
            </Button>
          )}
        </SetupFieldRow>
      </SetupField>
      {result ? (
        <p className={result.ok ? "nt-key-ok" : "nt-key-bad"} role="status">
          <Icon icon={result.ok ? Check : CircleAlert} size="sm" />
          {result.message}
        </p>
      ) : (
        inUse &&
        status?.has_key && (
          <p className="nt-key-ok" role="status">
            <Icon icon={Check} size="sm" />
            Meetings are transcribed by {name}.
          </p>
        )
      )}
      <SetupNote>
        {hint} While you use it, meeting audio goes to {name} with your key. Recordings and notes
        stay on {THIS_DEVICE}.
      </SetupNote>
    </SetupFields>
  );
}
