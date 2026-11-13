/**
 * Settings rows shared by the AI and Transcription pages: an API key that goes straight to the
 * keychain (never shown again), and a text setting that saves on Enter or blur.
 */
import { Check, CircleAlert, KeyRound } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { toAppError } from "@/ipc/client";
import { Button, Field, Icon, SettingsRow } from "@/ui";

/** What saving a key found out. */
export interface KeyCheck {
  ok: boolean;
  message: string;
}

/** The key goes straight to the keychain and is never shown again; we only learn whether it works. */
export function KeyRow({
  hasKey,
  saving,
  clearing,
  onSave,
  onClear,
  label = "API key",
  description,
}: {
  hasKey: boolean;
  saving: boolean;
  clearing: boolean;
  /** Save and check the key; resolves with what the check found. Rejects when it was not saved. */
  onSave: (key: string) => Promise<KeyCheck>;
  onClear: () => void;
  label?: string;
  /** Shown when there is no result to report. */
  description?: string;
}) {
  const id = useId();
  const [key, setKey] = useState("");
  const [result, setResult] = useState<KeyCheck | null>(null);
  const submit = () => {
    const trimmed = key.trim();
    if (!trimmed) return;
    setResult(null);
    onSave(trimmed).then(
      (check) => {
        setKey("");
        setResult(check);
      },
      (error: unknown) => setResult({ ok: false, message: toAppError(error).message }),
    );
  };
  return (
    <SettingsRow
      label={label}
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
          (description ?? "Stored in your system keychain, never in Cilica's files.")
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
            variant="primary"
            disabled={!key.trim()}
            loading={saving}
            onClick={submit}
          >
            {hasKey ? "Replace key" : "Save key"}
          </Button>
          {hasKey && !key && (
            <Button size="sm" variant="ghost" loading={clearing} onClick={onClear}>
              Remove
            </Button>
          )}
        </span>
      }
    />
  );
}

/** A text setting that saves on Enter or blur. */
export function TextSetting({
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
          spellCheck={false}
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
