/**
 * Settings › Margin: which model provider answers, its API key (stored in the keychain, never shown
 * again), base URL and models, whether edits apply without asking, and the experimental Claude
 * Code bridge behind an opt-in with the policy warning (spec › AI providers and auth).
 */
import { Check, CircleAlert, KeyRound, ShieldAlert, Terminal } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useProviderStatus } from "@/features/assistant/hooks";
import { toAppError } from "@/ipc/client";
import type { ProviderKind } from "@/ipc/types";
import {
  Button,
  Chip,
  DialogActions,
  DialogDescription,
  DialogRoot,
  DialogSurface,
  DialogTitle,
  Field,
  Icon,
  SelectMenu,
  SettingsGroup,
  SettingsPage,
  SettingsRow,
  Spinner,
  Switch,
} from "@/ui";
import { useSettings, useUpdateSettings } from "../hooks";
import { useClaudeCode, useProviderKey } from "../system";
import { LocalModelsGroup } from "./LocalModels";

/** Settings › Margin › Delete conversations after. 0 keeps them. */
const CONVERSATION_RETENTION = [
  { value: "1", label: "1 day" },
  { value: "7", label: "7 days" },
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
  { value: "0", label: "Never" },
] as const;
type RetentionValue = (typeof CONVERSATION_RETENTION)[number]["value"];

const PROVIDERS = [
  { value: "anthropic", label: "Anthropic", hint: "Recommended. Your own API key." },
  {
    value: "openai_compat",
    label: "OpenAI-compatible",
    hint: "OpenAI, OpenRouter, Groq or LM Studio.",
  },
  { value: "ollama", label: "Ollama", hint: "Local models, no key. Quality varies." },
  {
    value: "claude_code",
    label: "Claude Code",
    hint: "Experimental. Uses your installed claude CLI.",
  },
] as const satisfies readonly { value: ProviderKind; label: string; hint: string }[];

const NEEDS_KEY: ReadonlySet<ProviderKind> = new Set(["anthropic", "openai_compat"]);
const NEEDS_URL: ReadonlySet<ProviderKind> = new Set(["openai_compat", "ollama"]);
const URL_HINT: Partial<Record<ProviderKind, string>> = {
  openai_compat: "https://api.openai.com/v1",
  ollama: "http://localhost:11434",
};

export function MarginPage() {
  const { data: settings } = useSettings();
  const { patch } = useUpdateSettings();
  const { data: status, isFetching, error: statusError } = useProviderStatus();
  const [bridgeDialog, setBridgeDialog] = useState(false);
  if (!settings) return null;
  const ai = settings.ai;

  const choose = (provider: ProviderKind) => {
    if (provider === ai.provider) return;
    if (provider === "claude_code") setBridgeDialog(true);
    // Ollama downloads nothing until the person asks, from the Local model list below.
    else patch("ai", { provider });
  };

  return (
    <SettingsPage
      title="AI"
      description="The assistant that writes meeting summaries and edits notes when you ask."
    >
      <SettingsGroup title="Model provider">
        <SettingsRow
          label="Provider"
          description={PROVIDERS.find((p) => p.value === ai.provider)?.hint}
          control={
            <SelectMenu
              label="Provider"
              value={ai.provider}
              options={PROVIDERS}
              onChange={choose}
            />
          }
        />
        <SettingsRow
          label="Status"
          control={
            isFetching && !status && !statusError ? (
              <Spinner label="Checking" />
            ) : status?.ready && status.kind === ai.provider ? (
              <Chip tone="success" icon={Check}>
                Ready
              </Chip>
            ) : (
              <Chip tone="warning" icon={CircleAlert}>
                Not ready
              </Chip>
            )
          }
          description={
            statusError
              ? "Margin could not check this provider. Try again in a moment."
              : status && !status.ready
                ? status.problem
                : undefined
          }
        />
        {NEEDS_KEY.has(ai.provider) && (
          <KeyRow kind={ai.provider} hasKey={status?.has_key ?? false} />
        )}
        {NEEDS_URL.has(ai.provider) && (ai.provider !== "ollama" || ai.base_url) && (
          <TextSetting
            label="Base URL"
            description="Where Margin sends requests for this provider."
            value={ai.base_url ?? ""}
            placeholder={URL_HINT[ai.provider] ?? ""}
            onSave={(v) => patch("ai", { base_url: v.trim() || null })}
          />
        )}
        {ai.provider === "claude_code" && (
          <ClaudeCodeRow onTurnOff={() => patch("ai", { provider: "anthropic" })} />
        )}
      </SettingsGroup>
      {ai.provider === "ollama" ? (
        <LocalModelsGroup activeModel={ai.model} />
      ) : (
        <SettingsGroup title="Models" footer="Names as your provider lists them.">
          <TextSetting
            label="Assistant and summaries"
            value={ai.model}
            placeholder="claude-sonnet-5-5"
            onSave={(model) => model.trim() && patch("ai", { model: model.trim() })}
          />
          <TextSetting
            label="Titles and suggestions"
            description="A small, fast model is enough."
            value={ai.fast_model}
            placeholder="claude-haiku-5-5"
            onSave={(fast_model) =>
              fast_model.trim() && patch("ai", { fast_model: fast_model.trim() })
            }
          />
        </SettingsGroup>
      )}
      <SettingsGroup
        title="Changes"
        footer="Every change is versioned and can be undone. Deleting, merging and moving many notes always ask first, whatever this says."
      >
        <SettingsRow
          id="auto-apply"
          label="Apply edits without asking"
          description="Margin's edits to a note apply at once instead of waiting for your review on a diff card."
          control={
            <Switch
              aria-labelledby="auto-apply"
              checked={ai.auto_apply_edits}
              onCheckedChange={(auto_apply_edits) => patch("ai", { auto_apply_edits })}
            />
          }
        />
      </SettingsGroup>
      <SettingsGroup title="Conversations">
        <SettingsRow
          label="Delete conversations after"
          description="Counted from the last message. Changes Margin made to your notes stay, and stay in Margin activity."
          control={
            <SelectMenu<RetentionValue>
              label="Delete conversations after"
              value={retentionValue(ai.conversation_retention_days)}
              options={CONVERSATION_RETENTION}
              onChange={(v) => patch("ai", { conversation_retention_days: Number(v) })}
            />
          }
        />
      </SettingsGroup>
      <ClaudeCodeDialog
        open={bridgeDialog}
        onOpenChange={setBridgeDialog}
        onConfirm={() => {
          patch("ai", { provider: "claude_code" });
          setBridgeDialog(false);
        }}
      />
    </SettingsPage>
  );
}

/** The option for a stored day count; a count the menu does not offer shows as the default. */
function retentionValue(days: number): RetentionValue {
  return CONVERSATION_RETENTION.find((o) => o.value === String(days))?.value ?? "7";
}

/** A text setting that saves on Enter or blur. */
function TextSetting({
  label,
  description,
  value,
  placeholder,
  onSave,
}: {
  label: string;
  description?: string;
  value: string;
  placeholder: string;
  onSave: (value: string) => void;
}) {
  const id = useId();
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const commit = () => draft !== value && onSave(draft);
  return (
    <SettingsRow
      label={label}
      htmlFor={id}
      description={description}
      control={
        <Field
          id={id}
          size="sm"
          className="nt-setting-field"
          value={draft}
          placeholder={placeholder}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              commit();
            }
          }}
        />
      }
    />
  );
}

/** The key goes straight to the keychain and is never shown again; we only learn whether it works. */
function KeyRow({ kind, hasKey }: { kind: ProviderKind; hasKey: boolean }) {
  const id = useId();
  const { save, clear } = useProviderKey();
  const [key, setKey] = useState("");
  const [result, setResult] = useState<{ ok: boolean; message: string } | null>(null);
  const submit = () => {
    const trimmed = key.trim();
    if (!trimmed) return;
    setResult(null);
    save.mutate(
      { kind, key: trimmed },
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
        },
        onError: (error) => setResult({ ok: false, message: toAppError(error).message }),
      },
    );
  };
  return (
    <SettingsRow
      label="API key"
      htmlFor={id}
      description={
        result ? (
          <span className={result.ok ? "nt-key-ok" : "nt-key-bad"} role="status">
            <Icon icon={result.ok ? Check : CircleAlert} size="sm" />
            {result.message}
          </span>
        ) : hasKey ? (
          "A key is saved in your keychain. Paste a new one to replace it."
        ) : (
          "Stored in your system keychain, never in Margin's files."
        )
      }
      control={
        <span className="nt-key-control">
          <Field
            id={id}
            size="sm"
            type="password"
            icon={KeyRound}
            className="nt-setting-field"
            value={key}
            placeholder={hasKey ? "Saved in keychain" : "Paste your key"}
            autoComplete="off"
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
            variant="primary"
            disabled={!key.trim()}
            loading={save.isPending}
            onClick={submit}
          >
            {hasKey ? "Replace key" : "Save key"}
          </Button>
          {hasKey && !key && (
            <Button
              size="sm"
              variant="ghost"
              loading={clear.isPending}
              onClick={() => clear.mutate(kind)}
            >
              Remove
            </Button>
          )}
        </span>
      }
    />
  );
}

function ClaudeCodeRow({ onTurnOff }: { onTurnOff: () => void }) {
  const { data: cc, isLoading, refetch, isFetching } = useClaudeCode(true);
  return (
    <SettingsRow
      label={
        <span className="nt-model-name">
          Claude Code bridge
          <Chip tone="warning" icon={ShieldAlert}>
            Experimental
          </Chip>
        </span>
      }
      description={
        isLoading
          ? "Looking for claude on this Mac…"
          : !cc?.installed
            ? "Claude Code is not installed. Install it and sign in, then check again."
            : !cc.signed_in
              ? `Found claude ${cc.version ?? ""}, but it is not signed in. Run claude in a terminal and sign in.`
              : `Found claude ${cc.version ?? ""}, signed in. Margin never reads or stores its credentials.`
      }
      control={
        <span className="nt-key-control">
          <Button
            size="sm"
            variant="secondary"
            icon={Terminal}
            loading={isFetching}
            onClick={() => void refetch()}
          >
            Check again
          </Button>
          <Button size="sm" variant="ghost" onClick={onTurnOff}>
            Use an API key instead
          </Button>
        </span>
      }
    />
  );
}

function ClaudeCodeDialog({
  open,
  onOpenChange,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}) {
  const { data: cc } = useClaudeCode(open);
  return (
    <DialogRoot open={open} onOpenChange={onOpenChange}>
      <DialogSurface className="nt-dialog-wide">
        <DialogTitle>Use Claude Code for Margin?</DialogTitle>
        <DialogDescription asChild>
          <div className="nt-confirm-body">
            <p>
              Margin will run your installed <code>claude</code> command to answer, limited to
              Margin's own tools: no shell and no file edits. Margin never reads or stores your
              Claude credentials.
            </p>
            <p className="nt-policy">
              <Icon icon={ShieldAlert} size="sm" />
              <span>
                Anthropic's consumer terms say Free, Pro and Max subscriptions may only be used in
                Claude Code and claude.ai, and subscription access was cut off for third-party tools
                in April 2026. Running it from Margin may breach those terms and put your account at
                risk. Check Anthropic's current terms first. An API key is the supported way.
              </span>
            </p>
            <p className="nt-faint">
              {cc
                ? cc.installed
                  ? `Found claude ${cc.version ?? ""}${cc.signed_in ? ", signed in." : ", not signed in yet."}`
                  : "Claude Code is not installed on this Mac."
                : "Looking for Claude Code…"}
            </p>
          </div>
        </DialogDescription>
        <DialogActions>
          <Button variant="secondary" onClick={() => onOpenChange(false)}>
            Keep my API key
          </Button>
          <Button variant="primary" onClick={onConfirm}>
            Use Claude Code
          </Button>
        </DialogActions>
      </DialogSurface>
    </DialogRoot>
  );
}
