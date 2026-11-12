/** Settings › Transcription: local speech models (download, cancel, delete, choose) and language. */
import { Check, Download, Trash2, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { tweens } from "@/app/motion";
import type { SttModel } from "@/ipc/types";
import {
  Button,
  Chip,
  IconButton,
  Progress,
  SelectMenu,
  SettingsGroup,
  SettingsPage,
  SettingsRow,
  SkeletonText,
  Tooltip,
} from "@/ui";
import { useSettings, useUpdateSettings } from "../hooks";
import {
  type Download as DownloadState,
  formatBytes,
  useDownloads,
  useModelActions,
  useSttModels,
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

export function TranscriptionPage() {
  const { data: settings } = useSettings();
  const { patch } = useUpdateSettings();
  const { data: models, isLoading } = useSttModels();
  const downloads = useDownloads((s) => s.downloads);
  if (!settings) return null;
  const t = settings.transcription;
  const active = models?.find((m) => m.id === t.model_id);
  const englishOnly = active ? !active.multilingual : false;

  return (
    <SettingsPage
      title="Transcription"
      description="Speech is turned into text on this device. Audio never leaves it."
    >
      <SettingsGroup
        title="Speech model"
        footer="Larger models are more accurate and slower. Models download once and work offline."
      >
        {isLoading || !models ? (
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
              onUse={() => patch("transcription", { model_id: model.id, engine: "whisper_local" })}
            />
          ))
        )}
      </SettingsGroup>
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
    </SettingsPage>
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
  const { download: start, cancel, remove } = useModelActions();
  const fraction = download?.total ? (download.downloaded ?? 0) / download.total : null;
  return (
    <SettingsRow
      label={
        <span className="nt-model-name">
          {model.name}
          {model.recommended && <Chip tone="accent">Recommended</Chip>}
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
              <>
                <span className="nt-model-progress">
                  <Progress value={fraction} label={`Downloading ${model.name}`} />
                  <span className="nt-faint nt-tnum">
                    {fraction === null ? "Starting…" : `${Math.round(fraction * 100)}%`}
                  </span>
                </span>
                <IconButton
                  icon={X}
                  iconMotion="x"
                  label="Cancel download"
                  onClick={() => cancel.mutate(model.id)}
                />
              </>
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
              <Button
                size="sm"
                variant="secondary"
                icon={Download}
                iconMotion="up"
                onClick={() => start.mutate(model.id)}
              >
                Download
              </Button>
            )}
          </motion.span>
        </AnimatePresence>
      }
    />
  );
}
