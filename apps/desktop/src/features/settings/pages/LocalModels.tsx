/**
 * Settings › Margin › Local model, shown when Ollama is the provider: set up Ollama, then download,
 * cancel, delete and choose a model, like Settings › Transcription does for speech models.
 */
import { Check, CircleAlert, Download, Trash2, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { tweens } from "@/app/motion";
import type { LocalLlmModel } from "@/ipc/types";
import {
  Button,
  Chip,
  Icon,
  IconButton,
  Progress,
  SettingsGroup,
  SettingsRow,
  SkeletonText,
  Tooltip,
} from "@/ui";
import {
  type Download as DownloadState,
  formatBytes,
  OLLAMA_APP,
  OLLAMA_PREFIX,
  useDownloads,
  useLocalModelActions,
  useLocalModels,
  useOllamaStatus,
} from "../system";

export function LocalModelsGroup({ activeModel }: { activeModel: string }) {
  const { data: status, isLoading: statusLoading } = useOllamaStatus(true);
  const running = status?.running ?? false;
  const { data: models, isLoading } = useLocalModels(running);
  const downloads = useDownloads((s) => s.downloads);
  const { setUp } = useLocalModelActions();
  const setup = downloads[OLLAMA_APP];
  const installing = setup && !setup.error ? setup : undefined;
  const setupError = setup?.error ?? null;

  if (statusLoading) {
    return (
      <SettingsGroup title="Local model">
        <div className="nt-settings-row">
          <SkeletonText lines={2} />
        </div>
      </SettingsGroup>
    );
  }

  if (!running) {
    const fraction = installing?.total ? (installing.downloaded ?? 0) / installing.total : null;
    return (
      <SettingsGroup
        title="Local model"
        footer="Ollama runs models on this Mac. Notes and transcripts never leave it."
      >
        <SettingsRow
          label="Ollama"
          description={
            setupError ? (
              <span className="nt-key-bad" role="status">
                <Icon icon={CircleAlert} size="sm" />
                {setupError}
              </span>
            ) : installing ? (
              status?.installed ? (
                "Starting Ollama…"
              ) : (
                "Downloading Ollama. You pick a model next."
              )
            ) : status?.installed ? (
              "Ollama is installed but not running. Margin can start it."
            ) : (
              "Margin downloads and starts Ollama. You pick a model next."
            )
          }
          control={
            installing ? (
              <span className="nt-model-progress">
                <Progress value={fraction} label="Setting up Ollama" />
                <span className="nt-faint nt-tnum">
                  {fraction === null ? "Starting…" : `${Math.round(fraction * 100)}%`}
                </span>
              </span>
            ) : (
              <Button
                size="sm"
                variant="primary"
                icon={Download}
                iconMotion="up"
                onClick={() => setUp.mutate()}
              >
                {setupError ? "Try again" : status?.installed ? "Start Ollama" : "Download Ollama"}
              </Button>
            )
          }
        />
      </SettingsGroup>
    );
  }

  return (
    <SettingsGroup
      title="Local model"
      footer={`Runs on this Mac through Ollama ${status?.version ?? ""}. Larger models write better notes and need more memory.`}
    >
      {isLoading || !models ? (
        <div className="nt-settings-row">
          <SkeletonText lines={3} />
        </div>
      ) : (
        models.map((model) => (
          <LocalModelRow
            key={model.id}
            model={model}
            active={model.id === activeModel}
            download={downloads[OLLAMA_PREFIX + model.id]}
          />
        ))
      )}
    </SettingsGroup>
  );
}

function LocalModelRow({
  model,
  active,
  download: state,
}: {
  model: LocalLlmModel;
  active: boolean;
  download: DownloadState | undefined;
}) {
  const { pull, cancel, remove, choose } = useLocalModelActions();
  const failed = state?.error ?? null;
  const download = failed ? undefined : state;
  const fraction = download?.total ? (download.downloaded ?? 0) / download.total : null;
  const facts = [
    model.size_mb ? formatBytes(model.size_mb * 1024 * 1024) : null,
    model.min_ram_gb ? `${model.min_ram_gb} GB memory or more` : null,
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <SettingsRow
      label={
        <span className="nt-model-name">
          {model.name}
          {model.recommended && <Chip tone="accent">Recommended</Chip>}
        </span>
      }
      description={
        failed ? (
          <span className="nt-key-bad" role="status">
            <Icon icon={CircleAlert} size="sm" />
            {failed}
          </span>
        ) : (
          <>
            {model.description} {facts}
          </>
        )
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
                  <Button size="sm" variant="secondary" onClick={() => choose.mutate(model.id)}>
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
                onClick={() => pull.mutate(model.id)}
              >
                {failed ? "Try again" : "Download"}
              </Button>
            )}
          </motion.span>
        </AnimatePresence>
      }
    />
  );
}
