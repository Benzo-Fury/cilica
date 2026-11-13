/**
 * Step 3: where Cilica's language model runs. Anthropic with a key, Ollama on this computer, any
 * OpenAI-compatible server, or the experimental Claude Code bridge. Keys go to the keychain and
 * are checked before they are kept; nothing here changes the provider until it works.
 */
import { useQueryClient } from "@tanstack/react-query";
import { Check, CircleAlert, Download, Globe, KeyRound, ShieldAlert, Type } from "lucide-react";
import { type ReactNode, useId, useState } from "react";
import { qk } from "@/app/queryKeys";
import { useProviderStatus } from "@/features/assistant/hooks";
import { useSettings, useUpdateSettings } from "@/features/settings/hooks";
import {
  OLLAMA_APP,
  OLLAMA_PREFIX,
  useClaudeCode,
  useDownloads,
  useLocalModelActions,
  useLocalModels,
  useOllamaStatus,
  useProviderKey,
} from "@/features/settings/system";
import { api, toAppError } from "@/ipc/client";
import type { ProviderKind, Settings } from "@/ipc/types";
import {
  Button,
  Chip,
  Field,
  Icon,
  Progress,
  SetupCard,
  SetupChoice,
  SetupChoices,
  SetupField,
  SetupFieldRow,
  SetupFields,
  SetupNote,
  SetupStep,
  SetupWarning,
} from "@/ui";
import { fraction, type StepProps, THIS_DEVICE } from "./shared";

/** Anthropic's defaults, set again when switching to Anthropic from another provider. */
const ANTHROPIC_MODELS = { model: "claude-sonnet-5-5", fast_model: "claude-haiku-5-5" } as const;

type Result = { ok: boolean; message: string } | null;

function Message({ result }: { result: Result }) {
  if (!result) return null;
  return (
    <p className={result.ok ? "nt-key-ok" : "nt-key-bad"} role="status">
      <Icon icon={result.ok ? Check : CircleAlert} size="sm" />
      {result.message}
    </p>
  );
}

/** The host of a web address, or null when it is not one. */
export function hostOf(url: string): string | null {
  try {
    const parsed = new URL(url.trim());
    return parsed.protocol === "http:" || parsed.protocol === "https:" ? parsed.host : null;
  } catch {
    return null;
  }
}

export function ModelStep({ status, skipped, onNext, onReopen }: StepProps) {
  const { data: settings } = useSettings();
  const ai = settings?.ai;
  const [choice, setChoice] = useState<ProviderKind>(ai?.provider ?? "anthropic");
  const { data: provider } = useProviderStatus();
  const downloads = useDownloads((s) => s.downloads);
  const { data: localModels } = useLocalModels(ai?.provider === "ollama");

  const ready = Boolean(provider?.ready && provider.kind === ai?.provider);
  const pulling = ai?.provider === "ollama" ? downloads[OLLAMA_PREFIX + ai.model] : undefined;
  // Ollama and Claude Code count as set up once chosen: a model may still be downloading.
  const configured = ready || ai?.provider === "ollama" || ai?.provider === "claude_code";
  const localName = localModels?.find((m) => m.id === ai?.model)?.name ?? ai?.model;

  const summary = !configured
    ? "Skipped. Summaries wait until you add a model"
    : ai?.provider === "anthropic"
      ? "Anthropic connected · key in your keychain"
      : ai?.provider === "openai_compat"
        ? `Connected to ${hostOf(ai.base_url ?? "") ?? "your server"}`
        : ai?.provider === "ollama"
          ? pulling
            ? `Ollama · downloading ${localName} ${Math.round((fraction(pulling) ?? 0) * 100)}%`
            : `Ollama with ${localName}`
          : "Claude Code connected";

  return (
    <SetupStep
      id="model"
      status={status}
      title="Connect Cilica to a model"
      summary={summary}
      skipped={skipped && !configured}
      onReopen={onReopen}
      onSkip={() => onNext(!configured)}
      description="Summaries and the assistant need a language model. Choose where it runs."
      actions={
        <Button variant={configured ? "primary" : "secondary"} onClick={() => onNext(!configured)}>
          {configured ? "Continue" : "Skip for now"}
        </Button>
      }
    >
      <SetupChoices label="Model provider">
        <SetupChoice
          title="Anthropic"
          description="Claude, with your API key."
          checked={choice === "anthropic"}
          onSelect={() => setChoice("anthropic")}
        />
        <SetupChoice
          title="Ollama"
          badge={<Chip>Runs locally</Chip>}
          description={`Nothing leaves ${THIS_DEVICE}.`}
          checked={choice === "ollama"}
          onSelect={() => setChoice("ollama")}
        />
        <SetupChoice
          title="OpenAI-compatible"
          description="Any server with that API."
          checked={choice === "openai_compat"}
          onSelect={() => setChoice("openai_compat")}
        />
        <SetupChoice
          title="Claude Code"
          badge={<Chip tone="warning">Experimental</Chip>}
          description="Uses the claude command."
          checked={choice === "claude_code"}
          onSelect={() => setChoice("claude_code")}
        />
      </SetupChoices>
      {settings && choice === "anthropic" && <AnthropicKey settings={settings} />}
      {settings && choice === "openai_compat" && <OpenAiServer settings={settings} />}
      {choice === "ollama" && (
        <OllamaSetup activeModel={ai?.provider === "ollama" ? ai.model : null} />
      )}
      {choice === "claude_code" && <ClaudeCodeSetup active={ai?.provider === "claude_code"} />}
    </SetupStep>
  );
}

/** Save the settings document now (not optimistically): the key check reads it. */
function useSaveAi() {
  const queryClient = useQueryClient();
  return async (settings: Settings, ai: Partial<Settings["ai"]>) => {
    const saved = await api.updateSettings({ ...settings, ai: { ...settings.ai, ...ai } });
    queryClient.setQueryData(qk.settings, saved);
    return saved;
  };
}

function KeyField({
  id,
  value,
  placeholder,
  onChange,
  onSubmit,
  button,
}: {
  id: string;
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  button: ReactNode;
}) {
  return (
    <SetupFieldRow>
      <Field
        id={id}
        type="password"
        icon={KeyRound}
        value={value}
        placeholder={placeholder}
        autoComplete="off"
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            onSubmit();
          }
        }}
      />
      {button}
    </SetupFieldRow>
  );
}

function AnthropicKey({ settings }: { settings: Settings }) {
  const id = useId();
  const { save } = useProviderKey();
  const saveAi = useSaveAi();
  const [key, setKey] = useState("");
  const [result, setResult] = useState<Result>(null);
  const submit = () => {
    const trimmed = key.trim();
    if (!trimmed || save.isPending) return;
    setResult(null);
    save.mutate(
      { kind: "anthropic", key: trimmed },
      {
        onSuccess: (status) => {
          setKey("");
          setResult(
            status.ready
              ? { ok: true, message: "The key works. It is saved in your keychain." }
              : {
                  ok: false,
                  message:
                    status.problem ??
                    "The key was saved but did not work. Check it and paste it again.",
                },
          );
          if (settings.ai.provider !== "anthropic") {
            void saveAi(settings, { provider: "anthropic", base_url: null, ...ANTHROPIC_MODELS });
          }
        },
        onError: (error) => setResult({ ok: false, message: toAppError(error).message }),
      },
    );
  };
  return (
    <SetupFields>
      <SetupField label="API key" htmlFor={id}>
        <KeyField
          id={id}
          value={key}
          placeholder="sk-ant-…"
          onChange={setKey}
          onSubmit={submit}
          button={
            <Button
              variant="primary"
              disabled={!key.trim()}
              loading={save.isPending}
              onClick={submit}
            >
              Save key
            </Button>
          }
        />
      </SetupField>
      <Message result={result} />
      <SetupNote>The key goes into your keychain and is never shown again.</SetupNote>
    </SetupFields>
  );
}

function OpenAiServer({ settings }: { settings: Settings }) {
  const urlId = useId();
  const modelId = useId();
  const keyId = useId();
  const { save } = useProviderKey();
  const saveAi = useSaveAi();
  const isCurrent = settings.ai.provider === "openai_compat";
  const [url, setUrl] = useState(isCurrent ? (settings.ai.base_url ?? "") : "");
  const [model, setModel] = useState(isCurrent ? settings.ai.model : "");
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<Result>(null);
  const filled = Boolean(url.trim() && model.trim() && key.trim());

  const connect = async () => {
    if (!filled || busy) return;
    const host = hostOf(url);
    if (!host) {
      setResult({
        ok: false,
        message: "That is not a web address. It should start with https://.",
      });
      return;
    }
    setBusy(true);
    setResult(null);
    try {
      await saveAi(settings, {
        provider: "openai_compat",
        base_url: url.trim(),
        model: model.trim(),
        fast_model: model.trim(),
      });
      const status = await save.mutateAsync({ kind: "openai_compat", key: key.trim() });
      setKey("");
      setResult(
        status.ready
          ? { ok: true, message: `Connected to ${host}. The key is saved in your keychain.` }
          : {
              ok: false,
              message: status.problem ?? `${host} did not answer. Check the address and try again.`,
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
      <SetupField label="Server address" htmlFor={urlId}>
        <Field
          id={urlId}
          type="url"
          icon={Globe}
          value={url}
          placeholder="https://api.example.com/v1"
          autoComplete="off"
          onChange={(e) => setUrl(e.target.value)}
        />
      </SetupField>
      <SetupField label="Model" htmlFor={modelId}>
        <Field
          id={modelId}
          icon={Type}
          value={model}
          placeholder="The model's name, as your server lists it"
          autoComplete="off"
          onChange={(e) => setModel(e.target.value)}
        />
      </SetupField>
      <SetupField label="API key" htmlFor={keyId}>
        <KeyField
          id={keyId}
          value={key}
          placeholder="Paste your key"
          onChange={setKey}
          onSubmit={() => void connect()}
          button={
            <Button
              variant="primary"
              disabled={!filled}
              loading={busy}
              onClick={() => void connect()}
            >
              Connect
            </Button>
          }
        />
      </SetupField>
      <Message result={result} />
      <SetupNote>
        Works with OpenAI, OpenRouter, LM Studio and other servers that use the OpenAI API. The key
        goes into your keychain.
      </SetupNote>
    </SetupFields>
  );
}

function OllamaSetup({ activeModel }: { activeModel: string | null }) {
  const { data: status, isLoading } = useOllamaStatus(true);
  const running = status?.running ?? false;
  const { data: models } = useLocalModels(running);
  const downloads = useDownloads((s) => s.downloads);
  const { setUp, pull, choose } = useLocalModelActions();
  const install = downloads[OLLAMA_APP];

  if (isLoading) return <SetupNote>Looking for Ollama…</SetupNote>;

  if (!running) {
    return (
      <SetupCard
        title="Ollama"
        description={
          install?.error
            ? `${install.error} Try again, or install Ollama yourself from ollama.com.`
            : status?.installed
              ? "Ollama is installed but not running. Cilica can start it."
              : `Cilica downloads Ollama, then a model that fits ${THIS_DEVICE}.`
        }
        progress={
          install &&
          !install.error && <Progress value={fraction(install)} label="Setting up Ollama" />
        }
        action={
          !install || install.error ? (
            <Button variant="primary" onClick={() => setUp.mutate()}>
              {install?.error ? "Try again" : status?.installed ? "Start Ollama" : "Set up Ollama"}
            </Button>
          ) : null
        }
      />
    );
  }

  const model = models?.find((m) => m.recommended) ?? models?.[0];
  if (!model) return <SetupNote>Loading the models Cilica can use…</SetupNote>;
  const progress = downloads[OLLAMA_PREFIX + model.id];
  const inUse = activeModel === model.id;
  return (
    <SetupCard
      title={model.name}
      badge={<Chip tone="accent">Recommended</Chip>}
      description={`${model.description} ${(model.size_mb / 1024).toFixed(1)} GB, needs ${model.min_ram_gb} GB of memory.`}
      progress={
        progress &&
        !progress.error && (
          <Progress value={fraction(progress)} label={`Downloading ${model.name}`} />
        )
      }
      action={
        progress && !progress.error ? null : inUse && model.downloaded ? (
          <Chip tone="success" icon={Check}>
            In use
          </Chip>
        ) : model.downloaded ? (
          <Button variant="primary" onClick={() => choose.mutate(model.id)}>
            Use it
          </Button>
        ) : (
          <Button
            variant="primary"
            icon={Download}
            iconMotion="up"
            onClick={() => {
              pull.mutate(model.id);
              choose.mutate(model.id);
            }}
          >
            {progress?.error ? "Try again" : "Download"}
          </Button>
        )
      }
    />
  );
}

function ClaudeCodeSetup({ active }: { active: boolean }) {
  const { data: cc, isLoading, refetch, isFetching } = useClaudeCode(true);
  const { patch } = useUpdateSettings();
  const found = Boolean(cc?.installed);
  return (
    <>
      <SetupCard
        title={found ? "Found the claude command" : "Claude Code"}
        description={
          isLoading
            ? `Looking for claude on ${THIS_DEVICE}…`
            : !cc?.installed
              ? "Claude Code is not installed. Install it and sign in, then check again."
              : !cc.signed_in
                ? `Found claude ${cc.version ?? ""}, but it is not signed in. Run claude in a terminal and sign in.`
                : `Found claude ${cc.version ?? ""}, signed in. Cilica never reads or stores its credentials.`
        }
        action={
          active ? (
            <Chip tone="success" icon={Check}>
              In use
            </Chip>
          ) : found ? (
            <Button variant="primary" onClick={() => patch("ai", { provider: "claude_code" })}>
              Use Claude Code
            </Button>
          ) : (
            <Button variant="secondary" loading={isFetching} onClick={() => void refetch()}>
              Check again
            </Button>
          )
        }
      />
      <SetupWarning icon={ShieldAlert}>
        Anthropic's consumer terms say Free, Pro and Max subscriptions may only be used in Claude
        Code and claude.ai. Running it from Cilica may breach those terms and put your account at
        risk. An API key is the supported way.
      </SetupWarning>
    </>
  );
}
