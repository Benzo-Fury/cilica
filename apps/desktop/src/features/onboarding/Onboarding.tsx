/**
 * First launch (`onboarding_done` is false): welcome and the privacy promise, permissions, the
 * speech model download (it keeps going in the background), an optional API key, done. Steps
 * change with the gentle spring. Every step can be skipped and Esc leaves; nothing traps you.
 */
import {
  ArrowRight,
  AudioLines,
  Check,
  CircleAlert,
  Download,
  FileText,
  KeyRound,
  MicOff,
  ShieldCheck,
} from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { type ReactNode, useState } from "react";
import { useShortcut } from "@/app/keybinds";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { useSettings, useUpdateSettings } from "@/features/settings/hooks";
import { PermissionRows } from "@/features/settings/pages/PermissionRows";
import {
  formatBytes,
  useDownloads,
  useModelActions,
  usePermissions,
  useProviderKey,
  useSttModels,
} from "@/features/settings/system";
import { toAppError } from "@/ipc/client";
import {
  Button,
  Chip,
  DialogDescription,
  DialogRoot,
  DialogSurface,
  DialogTitle,
  Field,
  Icon,
  Kbd,
  Progress,
  StepDots,
} from "@/ui";

const STEPS = ["welcome", "permissions", "model", "key", "done"] as const;
type Step = (typeof STEPS)[number];

export function Onboarding() {
  const { data: settings } = useSettings();
  const { update, patch } = useUpdateSettings();
  const [closed, setClosed] = useState(false);
  const open = Boolean(settings) && settings?.onboarding_done === false && !closed;
  const finish = () => {
    setClosed(true);
    update((s) => ({ ...s, onboarding_done: true }));
  };
  return (
    <DialogRoot open={open} onOpenChange={(o) => !o && finish()}>
      {open && (
        <Flow
          onFinish={finish}
          onModel={(model_id) => patch("transcription", { model_id, engine: "whisper_local" })}
        />
      )}
    </DialogRoot>
  );
}

function Flow({ onFinish, onModel }: { onFinish: () => void; onModel: (id: string) => void }) {
  const reduced = useReducedMotion();
  const [index, setIndex] = useState(0);
  const [direction, setDirection] = useState<1 | -1>(1);
  const step: Step = STEPS[index] ?? "done";
  const go = (delta: 1 | -1) => {
    setDirection(delta);
    setIndex((i) => Math.min(STEPS.length - 1, Math.max(0, i + delta)));
  };
  const next = () => go(1);
  const x = reduced ? 0 : 24 * direction;
  return (
    <DialogSurface
      className="nt-onboarding"
      title="Set up Margin"
      onOpenAutoFocus={(e) => e.preventDefault()}
    >
      <div className="nt-onb-top">
        <StepDots count={STEPS.length} current={index} />
        {step !== "done" && (
          <Button size="sm" variant="ghost" onClick={onFinish}>
            Skip setup
          </Button>
        )}
      </div>
      <div className="nt-onb-stage">
        <AnimatePresence mode="popLayout" initial={false} custom={direction}>
          <motion.div
            key={step}
            className="nt-onb-step"
            initial={{ opacity: 0, x }}
            animate={{ opacity: 1, x: 0, transition: reduced ? reducedTransition : springs.gentle }}
            exit={{ opacity: 0, x: -x, transition: tweens.exit }}
          >
            {step === "welcome" && <Welcome onNext={next} />}
            {step === "permissions" && <Permissions onNext={next} onBack={() => go(-1)} />}
            {step === "model" && <Model onNext={next} onBack={() => go(-1)} onModel={onModel} />}
            {step === "key" && <ApiKey onNext={next} onBack={() => go(-1)} />}
            {step === "done" && <Done onFinish={onFinish} />}
          </motion.div>
        </AnimatePresence>
      </div>
    </DialogSurface>
  );
}

function StepFrame({
  icon,
  title,
  children,
  actions,
}: {
  icon: typeof Check;
  title: string;
  children: ReactNode;
  actions: ReactNode;
}) {
  return (
    <>
      <div className="nt-onb-icon">
        <Icon icon={icon} />
      </div>
      <DialogTitle className="nt-onb-title">{title}</DialogTitle>
      <div className="nt-onb-body">{children}</div>
      <div className="nt-onb-actions">{actions}</div>
    </>
  );
}

function Welcome({ onNext }: { onNext: () => void }) {
  return (
    <StepFrame
      icon={AudioLines}
      title="Welcome to Margin"
      actions={
        <Button variant="primary" size="lg" autoFocus onClick={onNext}>
          Get started
          <Icon icon={ArrowRight} size="sm" motion="arrow" />
        </Button>
      }
    >
      <DialogDescription className="nt-onb-lead">
        Margin records your meetings, writes the notes and keeps them next to everything else you
        write.
      </DialogDescription>
      <ul className="nt-promises">
        <li>
          <Icon icon={ShieldCheck} size="sm" />
          <span>
            <b>Audio stays on this Mac.</b> Speech is turned into text on this device.
          </span>
        </li>
        <li>
          <Icon icon={MicOff} size="sm" />
          <span>
            <b>No bot joins the call.</b> Margin listens to your microphone and the call audio.
          </span>
        </li>
        <li>
          <Icon icon={FileText} size="sm" />
          <span>
            <b>Your notes are plain Markdown.</b> Export any note whenever you like.
          </span>
        </li>
      </ul>
    </StepFrame>
  );
}

function Permissions({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  const { data: permissions } = usePermissions(true);
  const ready = permissions?.microphone === "granted" || permissions?.microphone === "not_needed";
  return (
    <StepFrame
      icon={ShieldCheck}
      title="Let Margin hear the meeting"
      actions={
        <>
          <Button variant="ghost" onClick={onBack}>
            Back
          </Button>
          <Button variant={ready ? "primary" : "secondary"} onClick={onNext}>
            {ready ? "Continue" : "Skip for now"}
          </Button>
        </>
      }
    >
      <DialogDescription className="nt-onb-lead">
        Your Mac asks once for each. You can change them later in System Settings.
      </DialogDescription>
      <PermissionRows permissions={permissions} />
    </StepFrame>
  );
}

function Model({
  onNext,
  onBack,
  onModel,
}: {
  onNext: () => void;
  onBack: () => void;
  onModel: (id: string) => void;
}) {
  const { data: models = [] } = useSttModels();
  const { download } = useModelActions();
  const downloads = useDownloads((s) => s.downloads);
  const model = models.find((m) => m.recommended) ?? models[0];
  const progress = model ? downloads[model.id] : undefined;
  const fraction = progress?.total ? (progress.downloaded ?? 0) / progress.total : null;
  return (
    <StepFrame
      icon={Download}
      title="Download the speech model"
      actions={
        <>
          <Button variant="ghost" onClick={onBack}>
            Back
          </Button>
          <Button
            variant={model?.downloaded || progress ? "primary" : "secondary"}
            onClick={onNext}
          >
            {model?.downloaded
              ? "Continue"
              : progress
                ? "Continue while it downloads"
                : "Skip for now"}
          </Button>
        </>
      }
    >
      <DialogDescription className="nt-onb-lead">
        Margin transcribes with a Whisper model that runs on this Mac, so audio never leaves it. It
        downloads once.
      </DialogDescription>
      {model && (
        <div className="nt-onb-card">
          <div className="nt-onb-card-text">
            <div className="nt-perm-title">
              {model.name} <Chip tone="accent">Recommended</Chip>
            </div>
            <div className="nt-perm-desc">
              {model.description} {formatBytes(model.size_mb * 1024 * 1024)}.
            </div>
            {progress && (
              <div className="nt-model-progress nt-onb-progress">
                <Progress value={fraction} label={`Downloading ${model.name}`} />
                <span className="nt-faint nt-tnum">
                  {fraction === null ? "Starting…" : `${Math.round(fraction * 100)}%`}
                </span>
              </div>
            )}
          </div>
          {model.downloaded ? (
            <Chip tone="success" icon={Check}>
              Downloaded
            </Chip>
          ) : !progress ? (
            <Button
              size="sm"
              variant="primary"
              icon={Download}
              iconMotion="up"
              onClick={() => {
                onModel(model.id);
                download.mutate(model.id);
              }}
            >
              Download
            </Button>
          ) : null}
        </div>
      )}
      <p className="nt-onb-note">
        Keep going while it downloads. Margin tells you when it is ready.
      </p>
    </StepFrame>
  );
}

function ApiKey({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  const { save } = useProviderKey();
  const [key, setKey] = useState("");
  const [result, setResult] = useState<{ ok: boolean; message: string } | null>(null);
  const submit = () => {
    if (!key.trim()) return;
    save.mutate(
      { kind: "anthropic", key: key.trim() },
      {
        onSuccess: (status) => {
          setKey("");
          setResult(
            status.ready
              ? { ok: true, message: "The key works. It is saved in your keychain." }
              : {
                  ok: false,
                  message: status.problem ?? "That key did not work. Check it and paste it again.",
                },
          );
        },
        onError: (error) => setResult({ ok: false, message: toAppError(error).message }),
      },
    );
  };
  return (
    <StepFrame
      icon={KeyRound}
      title="Connect Margin to a model"
      actions={
        <>
          <Button variant="ghost" onClick={onBack}>
            Back
          </Button>
          <Button variant={result?.ok ? "primary" : "secondary"} onClick={onNext}>
            {result?.ok ? "Continue" : "Skip for now"}
          </Button>
        </>
      }
    >
      <DialogDescription className="nt-onb-lead">
        Summaries and the assistant need an Anthropic API key. It goes into your system keychain and
        is never shown again. Optional: you can add it later in Settings › Margin.
      </DialogDescription>
      <div className="nt-onb-key">
        <Field
          type="password"
          icon={KeyRound}
          value={key}
          placeholder="sk-ant-…"
          aria-label="Anthropic API key"
          autoComplete="off"
          onChange={(e) => setKey(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              submit();
            }
          }}
        />
        <Button variant="primary" disabled={!key.trim()} loading={save.isPending} onClick={submit}>
          Save key
        </Button>
      </div>
      {result && (
        <p className={result.ok ? "nt-key-ok" : "nt-key-bad"} role="status">
          <Icon icon={result.ok ? Check : CircleAlert} size="sm" />
          {result.message}
        </p>
      )}
    </StepFrame>
  );
}

function Done({ onFinish }: { onFinish: () => void }) {
  const record = useShortcut("recording.toggle");
  const palette = useShortcut("palette.open");
  const assistant = useShortcut("assistant.toggle");
  return (
    <StepFrame
      icon={Check}
      title="You are set"
      actions={
        <Button variant="primary" size="lg" autoFocus onClick={onFinish}>
          Start using Margin
        </Button>
      }
    >
      <DialogDescription className="nt-onb-lead">
        Three shortcuts get you most of the way.
      </DialogDescription>
      <ul className="nt-onb-keys">
        <li>
          <span>Start or stop recording, from anywhere</span>
          <Kbd keys={record} />
        </li>
        <li>
          <span>Find a note or run a command</span>
          <Kbd keys={palette} />
        </li>
        <li>
          <span>Ask Margin</span>
          <Kbd keys={assistant} />
        </li>
      </ul>
    </StepFrame>
  );
}
