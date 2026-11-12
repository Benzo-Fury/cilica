/**
 * Settings › Transcription: where speech is turned into text (a Whisper model on this computer,
 * or a cloud service with the person's own key), the careful pass after the meeting, and the
 * spoken language.
 */
import { Check, CircleAlert, Cloud, Download, Laptop, Trash2, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { tweens } from "@/app/motion";
import { toAppError } from "@/ipc/client";
import type { SttModel } from "@/ipc/types";
import {
  Button,
  Chip,
  DialogActions,
  DialogDescription,
  DialogRoot,
  DialogSurface,
  DialogTitle,
  Icon,
  IconButton,
  Progress,
  SegmentedControl,
  SelectMenu,
  SettingsGroup,
  SettingsPage,
  SettingsRow,
  SkeletonText,
  Spinner,
  Switch,
  Tooltip,
} from "@/ui";
import { KeyRow, TextSetting } from "../fields";
import { useSettings, useUpdateSettings } from "../hooks";
import {
  CLOUD_PROVIDERS,
  type CloudEngine,
  firstWordsLabel,
  hasLiveSpeakers,
  liveSpeakersHint,
  providerName,
} from "../speech";
import {
  type Download as DownloadState,
  formatBytes,
  useDownloads,
  useHardwareProfile,
  useModelActions,
  useSttModels,
  useSttProviderKey,
  useSttProviderStatus,
  useTranscriptionTest,
} from "../system";

export const LANGUAGES = [
  { value: "auto", label: "Detect automatically" },
  { value: "en", label: "English" },
  { value: "es", label: "Spanish" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
  { value: "it", label: "Italian" },
  { value: "pt", label: "Portuguese" },
  { value: "nl", label: "Dutch" },
  { value: "sv", label: "Swedish" },
  { value: "da", label: "Danish" },
  { value: "no", label: "Norwegian" },
  { value: "fi", label: "Finnish" },
  { value: "pl", label: "Polish" },
  { value: "ja", label: "Japanese" },
  { value: "ko", label: "Korean" },
  { value: "zh", label: "Chinese" },
] as const;

/** The model the careful pass after the meeting uses. */
const REFINE_MODEL = "large-v3-turbo-q5_0";

type Where = "local" | "cloud";

export function TranscriptionPage() {
  const { data: settings } = useSettings();
  const { patch } = useUpdateSettings();
  const { data: models, isLoading } = useSttModels();
  const { data: hardware } = useHardwareProfile();
  const downloads = useDownloads((s) => s.downloads);
  const [confirming, setConfirming] = useState<CloudEngine | null>(null);
  if (!settings) return null;
  const t = settings.transcription;
  const where: Where = t.engine === "whisper_local" ? "local" : "cloud";
  const active = models?.find((m) => m.id === t.model_id);
  const englishOnly = where === "local" && active ? !active.multilingual : false;
  const refineModel = models?.find((m) => m.id === REFINE_MODEL);

  const chooseWhere = (next: Where) => {
    if (next === where) return;
    if (next === "local") patch("transcription", { engine: "whisper_local" });
    else setConfirming("soniox");
  };
  const chooseCloud = (engine: CloudEngine) => {
    setConfirming(null);
    patch("transcription", { engine });
  };

  return (
    <SettingsPage
      title="Transcription"
      description={
        where === "local"
          ? "Speech is turned into text on this computer. Audio never leaves it."
          : `Speech is sent to ${providerName(t.engine)} to be turned into text, with your own key. Recordings and notes stay on this computer.`
      }
    >
      <SettingsGroup
        title="Live transcription"
        footer={
          where === "local"
            ? hardware?.suggest_cloud
              ? "This computer may struggle to keep up with a live transcript. A cloud service gives better results for a few dollars a month."
              : "Larger models are more accurate and slower. Models download once and work offline."
            : "You pay the service directly. Prices are approximate; check the service's own pricing."
        }
      >
        <SettingsRow
          label="Transcribe on"
          description={
            where === "local"
              ? "Private and free. Quality depends on this computer."
              : "Better live text on any computer. Audio goes to the service you pick."
          }
          control={
            <SegmentedControl<Where>
              label="Where speech is transcribed"
              value={where}
              onChange={chooseWhere}
              options={[
                { value: "local", label: "This computer", icon: Laptop },
                { value: "cloud", label: "Cloud service", icon: Cloud },
              ]}
            />
          }
        />
        {where === "local" ? (
          isLoading || !models ? (
            <div className="nt-settings-row">
              <SkeletonText lines={3} />
            </div>
          ) : (
            models.map((model) => (
              <ModelRow
                key={model.id}
                model={model}
                active={model.id === t.model_id}
                download={downloads[model.id]}
                onUse={() =>
                  patch("transcription", { model_id: model.id, engine: "whisper_local" })
                }
              />
            ))
          )
        ) : (
          <CloudSettings
            engine={t.engine as CloudEngine}
            baseUrl={t.cloud.base_url ?? ""}
            model={t.cloud.model ?? ""}
            onProvider={(engine) => setConfirming(engine)}
            onBaseUrl={(v) =>
              patch("transcription", { cloud: { ...t.cloud, base_url: v.trim() || null } })
            }
            onModel={(v) =>
              patch("transcription", { cloud: { ...t.cloud, model: v.trim() || null } })
            }
          />
        )}
      </SettingsGroup>

      {/* A cloud service's transcript is kept as it is: no second pass on this computer. */}
      {where === "local" && (
        <SettingsGroup title="After the meeting">
          <RefineRow
            on={t.refine_after_meeting}
            model={refineModel}
            download={refineModel ? downloads[refineModel.id] : undefined}
            onChange={(on) => patch("transcription", { refine_after_meeting: on })}
          />
        </SettingsGroup>
      )}

      <SpeakersGroup
        live={t.live_speaker_labels}
        supported={hasLiveSpeakers(t.engine)}
        hint={liveSpeakersHint(t.engine, t.identify_speakers_after_meeting)}
        identify={t.identify_speakers_after_meeting}
        recognise={t.recognise_voices}
        onLive={(on) => patch("transcription", { live_speaker_labels: on })}
        onIdentify={(on) => patch("transcription", { identify_speakers_after_meeting: on })}
        onRecognise={(on) => patch("transcription", { recognise_voices: on })}
      />

      <SettingsGroup title="Language">
        <SettingsRow
          label="Spoken language"
          description={
            englishOnly
              ? "The model in use only transcribes English. Choose a multilingual model for other languages."
              : "Detecting works for most meetings. Pick one if a meeting mixes languages."
          }
          control={
            <SelectMenu
              label="Spoken language"
              value={t.language ?? "auto"}
              options={LANGUAGES}
              disabled={englishOnly}
              onChange={(v) => patch("transcription", { language: v === "auto" ? null : v })}
            />
          }
        />
      </SettingsGroup>

      <CloudConsentDialog
        engine={confirming}
        fromCloud={where === "cloud"}
        onCancel={() => setConfirming(null)}
        onConfirm={chooseCloud}
      />
    </SettingsPage>
  );
}

/** Who is speaking: live labels from the service (where it can), and the check after the meeting. */
function SpeakersGroup({
  live,
  supported,
  hint,
  identify,
  recognise,
  onLive,
  onIdentify,
  onRecognise,
}: {
  live: boolean;
  supported: boolean;
  hint: string;
  identify: boolean;
  recognise: boolean;
  onLive: (on: boolean) => void;
  onIdentify: (on: boolean) => void;
  onRecognise: (on: boolean) => void;
}) {
  const liveOn = live && supported;
  return (
    <SettingsGroup title="Speakers">
      <SettingsRow
        label="Show who's speaking live"
        description={hint}
        control={
          <Switch
            checked={liveOn}
            disabled={!supported}
            onCheckedChange={onLive}
            label="Show who's speaking live"
          />
        }
      />
      <SettingsRow
        label="Identify speakers after the meeting"
        description={
          identify
            ? "Once the meeting ends, Margin tells voices apart on this computer, which is more accurate than live labels, and suggests names for voices it knows."
            : liveOn
              ? "Off: the live labels are kept as they are."
              : "Off: the transcript shows only You and Others."
        }
        control={
          <Switch
            checked={identify}
            onCheckedChange={onIdentify}
            label="Identify speakers after the meeting"
          />
        }
      />
      <SettingsRow
        label="Recognise saved voices"
        description={
          recognise
            ? "Names speakers who sound like someone in Saved people, including you, and asks when it's less sure. Pick the right person to teach it. Voiceprints stay on this computer; some places require people's consent before you keep one."
            : "Off: speakers stay Person 1, Person 2 and so on until you name them."
        }
        control={
          <Switch
            checked={recognise}
            onCheckedChange={onRecognise}
            label="Recognise saved voices"
          />
        }
      />
    </SettingsGroup>
  );
}

function ModelRow({
  model,
  active,
  download,
  onUse,
}: {
  model: SttModel;
  active: boolean;
  download: DownloadState | undefined;
  onUse: () => void;
}) {
  const { downloadAndUse, cancel, remove } = useModelActions();
  const fraction = download?.total ? (download.downloaded ?? 0) / download.total : null;
  return (
    <SettingsRow
      label={
        <span className="nt-model-name">
          {model.name}
          {model.recommended && <Chip tone="accent">Recommended for this computer</Chip>}
        </span>
      }
      description={
        <>
          {model.description} {formatBytes(model.size_mb * 1024 * 1024)}
          {model.multilingual ? " · Many languages" : " · English"}
        </>
      }
      control={
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span
            key={download ? "dl" : model.downloaded ? (active ? "active" : "ready") : "get"}
            className="nt-model-control"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: tweens.base }}
            exit={{ opacity: 0, transition: tweens.exit }}
          >
            {download ? (
              <DownloadProgress
                name={model.name}
                fraction={fraction}
                onCancel={() => cancel.mutate(model.id)}
              />
            ) : model.downloaded ? (
              <>
                {active ? (
                  <Chip tone="success" icon={Check}>
                    In use
                  </Chip>
                ) : (
                  <Button size="sm" variant="secondary" onClick={onUse}>
                    Use this model
                  </Button>
                )}
                <Tooltip
                  label={active ? "Choose another model before deleting this one" : "Delete"}
                  side="top"
                >
                  <span>
                    <IconButton
                      icon={Trash2}
                      iconMotion="trash"
                      label={`Delete ${model.name}`}
                      noTooltip
                      disabled={active}
                      onClick={() => remove.mutate(model.id)}
                    />
                  </span>
                </Tooltip>
              </>
            ) : (
              <Tooltip label="Downloads, then Margin switches to it" side="top">
                <Button
                  size="sm"
                  variant="secondary"
                  icon={Download}
                  iconMotion="up"
                  onClick={() => downloadAndUse.mutate(model.id)}
                >
                  Download
                </Button>
              </Tooltip>
            )}
          </motion.span>
        </AnimatePresence>
      }
    />
  );
}

function DownloadProgress({
  name,
  fraction,
  onCancel,
}: {
  name: string;
  fraction: number | null;
  onCancel: () => void;
}) {
  return (
    <>
      <span className="nt-model-progress">
        <Progress value={fraction} label={`Downloading ${name}`} />
        <span className="nt-faint nt-tnum">
          {fraction === null ? "Starting…" : `${Math.round(fraction * 100)}%`}
        </span>
      </span>
      <IconButton icon={X} iconMotion="x" label="Cancel download" onClick={onCancel} />
    </>
  );
}

function CloudSettings({
  engine,
  baseUrl,
  model,
  onProvider,
  onBaseUrl,
  onModel,
}: {
  engine: CloudEngine;
  baseUrl: string;
  model: string;
  onProvider: (engine: CloudEngine) => void;
  onBaseUrl: (value: string) => void;
  onModel: (value: string) => void;
}) {
  const { data: status, isLoading } = useSttProviderStatus(engine);
  const { save, clear } = useSttProviderKey(engine);
  const provider = CLOUD_PROVIDERS.find((p) => p.value === engine);
  const compatible = engine === "open_ai_compatible";
  const needsSetup = compatible && (!baseUrl || !model);
  return (
    <>
      <SettingsRow
        label="Service"
        description={provider?.hint}
        control={
          <SelectMenu
            label="Transcription service"
            value={engine}
            options={CLOUD_PROVIDERS}
            onChange={(next) => next !== engine && onProvider(next)}
          />
        }
      />
      {compatible && (
        <>
          <TextSetting
            label="Base URL"
            description="The address before /audio/transcriptions."
            value={baseUrl}
            placeholder="https://api.groq.com/openai/v1"
            onSave={onBaseUrl}
          />
          <TextSetting
            label="Model"
            value={model}
            placeholder="whisper-large-v3-turbo"
            onSave={onModel}
          />
        </>
      )}
      <SettingsRow
        label="Status"
        description={
          status?.problem ??
          (needsSetup
            ? "Add the base URL and model, then your key."
            : status?.has_key
              ? "Meetings are transcribed by this service."
              : "Add your API key to start.")
        }
        control={
          isLoading ? (
            <Spinner label="Checking" />
          ) : status?.has_key && !needsSetup && !status.problem ? (
            <Chip tone="success" icon={Check}>
              Ready
            </Chip>
          ) : (
            <Chip tone="warning" icon={CircleAlert}>
              Not ready
            </Chip>
          )
        }
      />
      <KeyRow
        key={engine}
        hasKey={status?.has_key ?? false}
        saving={save.isPending}
        clearing={clear.isPending}
        onSave={(key) =>
          save
            .mutateAsync(key)
            .then((s) =>
              s.problem
                ? { ok: false, message: s.problem }
                : { ok: true, message: "The key works. It is saved in your keychain." },
            )
        }
        onClear={() => clear.mutate()}
      />
      <TestRow
        key={`test-${engine}`}
        name={compatible ? "your service" : (provider?.label ?? "the service")}
        ready={Boolean(status?.has_key) && !needsSetup}
      />
    </>
  );
}

/** Streams the sample clip to the service in real time and shows what comes back. */
function TestRow({ name, ready }: { name: string; ready: boolean }) {
  const { run, heard } = useTranscriptionTest();
  const result = run.data;
  const description = run.isPending ? (
    <span role="status">{heard ? `“${heard}”` : `Sending the clip to ${name}…`}</span>
  ) : run.error ? (
    <span className="nt-key-bad" role="status">
      <Icon icon={CircleAlert} size="sm" />
      {toAppError(run.error).message}
    </span>
  ) : result ? (
    <span className="nt-key-ok" role="status">
      <Icon icon={Check} size="sm" />
      {`Heard “${result.text}” ${firstWordsLabel(result.first_words_ms)}`.trim()}
    </span>
  ) : ready ? (
    `Plays an 11-second public-domain speech clip to ${name}, as a meeting would, and shows what comes back. Costs well under a cent.`
  ) : (
    "Add your API key first."
  );
  return (
    <SettingsRow
      label="Test"
      description={description}
      control={
        <Button
          size="sm"
          variant="secondary"
          disabled={!ready}
          loading={run.isPending}
          onClick={() => run.mutate()}
        >
          {result || run.error ? "Test again" : "Run test"}
        </Button>
      }
    />
  );
}

function RefineRow({
  on,
  model,
  download,
  onChange,
}: {
  on: boolean;
  model: SttModel | undefined;
  download: DownloadState | undefined;
  onChange: (on: boolean) => void;
}) {
  const { download: start, cancel } = useModelActions();
  const missing = model ? !model.downloaded : false;
  const fraction = download?.total ? (download.downloaded ?? 0) / download.total : null;
  return (
    <SettingsRow
      label="Improve the transcript when the meeting ends"
      description={
        missing && on
          ? `Needs ${model?.name ?? "Whisper Large v3 Turbo"} on this computer (${formatBytes((model?.size_mb ?? 0) * 1024 * 1024)}). Until then the live transcript is kept.`
          : "Transcribes the whole recording again, carefully, on this computer. It catches words the live transcript missed and takes a few minutes after long meetings."
      }
      control={
        <span className="nt-model-control">
          {download && model ? (
            <DownloadProgress
              name={model.name}
              fraction={fraction}
              onCancel={() => cancel.mutate(model.id)}
            />
          ) : missing && on && model ? (
            <Button
              size="sm"
              variant="secondary"
              icon={Download}
              iconMotion="up"
              onClick={() => start.mutate(model.id)}
            >
              Download
            </Button>
          ) : null}
          <Switch
            checked={on}
            onCheckedChange={onChange}
            label="Improve the transcript when the meeting ends"
          />
        </span>
      }
    />
  );
}

function CloudConsentDialog({
  engine,
  fromCloud,
  onCancel,
  onConfirm,
}: {
  engine: CloudEngine | null;
  /** Switching between services rather than leaving this computer. */
  fromCloud: boolean;
  onCancel: () => void;
  onConfirm: (engine: CloudEngine) => void;
}) {
  const name = engine ? providerName(engine) : "";
  const label = engine === "open_ai_compatible" ? "the service you set up" : name;
  return (
    <DialogRoot open={engine !== null} onOpenChange={(open) => !open && onCancel()}>
      <DialogSurface>
        <DialogTitle>Send meeting audio to {label}?</DialogTitle>
        <DialogDescription asChild>
          <div className="nt-confirm-body">
            <p>
              While you use it, the audio of your meetings goes to {label} to be turned into text,
              with your own API key. Its terms and privacy policy apply.
            </p>
            <p className="nt-faint">
              Recordings, notes and transcripts stay on this computer. You can switch back to
              transcribing on this computer at any time.
            </p>
          </div>
        </DialogDescription>
        <DialogActions>
          <Button variant="secondary" onClick={onCancel}>
            {fromCloud ? "Cancel" : "Keep it on this computer"}
          </Button>
          <Button variant="primary" onClick={() => engine && onConfirm(engine)}>
            Use {engine === "open_ai_compatible" ? "this service" : name}
          </Button>
        </DialogActions>
      </DialogSurface>
    </DialogRoot>
  );
}
