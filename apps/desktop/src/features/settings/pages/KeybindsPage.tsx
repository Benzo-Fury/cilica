/** Settings › Keybinds: rebind any shortcut, with conflict detection and reset. */
import { RotateCcw, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import {
  ACTIONS,
  type ActionDef,
  effectiveKeys,
  eventToAccelerator,
  findConflict,
  formatAccelerator,
  isRebindable,
  isValidBinding,
  type KeybindGroup,
  listConflicts,
  rebind,
  resetBinding,
  useKeybindStore,
} from "@/app/keybinds";
import { useEscapeLayer } from "@/app/layers";
import { isMac } from "@/app/platform";
import { Button, Icon, IconButton, Kbd, SettingsGroup, SettingsPage, SettingsRow } from "@/ui";
import { useSettings, useUpdateSettings } from "../hooks";

const GROUPS: KeybindGroup[] = ["General", "Notes", "View", "Recording", "Editing"];

const SCOPE_HINT: Partial<Record<ActionDef["scope"], string>> = {
  global: "Works even when Cilica is in the background.",
  note: "While a note is open.",
  editor: "While typing in a note.",
};

type Pending = { actionId: string; keys: string; conflictWith?: ActionDef; error?: string };

function KeyCapture({
  action,
  overrides,
  capturing,
  onStart,
  onCaptured,
}: {
  action: ActionDef;
  overrides: Record<string, string>;
  capturing: boolean;
  onStart: () => void;
  onCaptured: (pending: Pending | null) => void;
}) {
  const setCapturing = useKeybindStore((s) => s.setCapturing);
  const current = effectiveKeys(action, overrides);

  useEscapeLayer(capturing, () => onCaptured(null));

  useEffect(() => {
    if (!capturing) return;
    setCapturing(true);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") return; // the Esc layer cancels
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Backspace" && !event.metaKey && !event.ctrlKey && !event.altKey) {
        onCaptured({ actionId: action.id, keys: "" });
        return;
      }
      const keys = eventToAccelerator(event, isMac);
      if (!keys) return; // a lone modifier: keep listening
      if (!isValidBinding(keys)) {
        onCaptured({
          actionId: action.id,
          keys,
          error: `Add ${isMac ? "⌘, ⌃ or ⌥" : "Ctrl or Alt"} so typing never triggers it.`,
        });
        return;
      }
      onCaptured({
        actionId: action.id,
        keys,
        conflictWith: findConflict(action.id, keys, overrides),
      });
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      setCapturing(false);
    };
  }, [capturing, action.id, overrides, onCaptured, setCapturing]);

  return (
    <Button
      size="sm"
      variant="secondary"
      className="nt-kbd-capture"
      data-capturing={capturing || undefined}
      aria-label={`Shortcut for ${action.label}: ${current ? formatAccelerator(current, isMac).join(" ") : "none"}. Press to change.`}
      onClick={onStart}
    >
      {capturing ? (
        <span className="nt-faint">Type a shortcut</span>
      ) : current ? (
        <Kbd keys={formatAccelerator(current, isMac)} />
      ) : (
        <span className="nt-faint">Not set</span>
      )}
    </Button>
  );
}

export function KeybindsPage() {
  const { data: settings } = useSettings();
  const { update } = useUpdateSettings();
  const [capturing, setCapturing] = useState<string | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  if (!settings) return null;
  const overrides = settings.keybinds;
  const save = (next: Record<string, string>) => update((s) => ({ ...s, keybinds: next }));
  const conflicts = listConflicts(overrides);

  const onCaptured = (result: Pending | null) => {
    setCapturing(null);
    if (!result) return setPending(null);
    if (result.error || result.conflictWith) return setPending(result);
    setPending(null);
    save(rebind(overrides, result.actionId, result.keys || null));
  };

  return (
    <SettingsPage
      title="Keybinds"
      description={`Click a shortcut, then press the new keys. ${isMac ? "⌫" : "Backspace"} clears it, Esc cancels.`}
    >
      {conflicts.length > 0 && (
        <div className="nt-conflict" role="status">
          <Icon icon={TriangleAlert} size="sm" />
          Some shortcuts are used twice. The first in this list wins.
        </div>
      )}
      {GROUPS.map((group) => {
        const actions = ACTIONS.filter((a) => a.group === group && isRebindable(a));
        if (actions.length === 0) return null;
        return (
          <SettingsGroup key={group} title={group}>
            {actions.map((action) => {
              const overridden = Object.hasOwn(overrides, action.id);
              const mine = pending?.actionId === action.id ? pending : null;
              return (
                <SettingsRow
                  key={action.id}
                  label={action.label}
                  description={SCOPE_HINT[action.scope]}
                  control={
                    <>
                      {overridden && (
                        <IconButton
                          icon={RotateCcw}
                          iconMotion="undo"
                          size="sm"
                          label={`Reset ${action.label} to ${formatAccelerator(action.defaultKeys, isMac).join("") || "none"}`}
                          onClick={() => save(resetBinding(overrides, action.id))}
                        />
                      )}
                      <KeyCapture
                        action={action}
                        overrides={overrides}
                        capturing={capturing === action.id}
                        onStart={() => {
                          setPending(null);
                          setCapturing(action.id);
                        }}
                        onCaptured={onCaptured}
                      />
                    </>
                  }
                >
                  {mine?.error && (
                    <div className="nt-conflict" role="alert">
                      <Icon icon={TriangleAlert} size="sm" />
                      {mine.error}
                    </div>
                  )}
                  {mine?.conflictWith && (
                    <div className="nt-conflict" role="alert">
                      <Icon icon={TriangleAlert} size="sm" />
                      <span>
                        <Kbd keys={formatAccelerator(mine.keys, isMac)} /> is already used by{" "}
                        {mine.conflictWith.label}.
                      </span>
                      <Button size="sm" variant="ghost" onClick={() => setPending(null)}>
                        Cancel
                      </Button>
                      <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => {
                          save(rebind(overrides, mine.actionId, mine.keys, { replace: true }));
                          setPending(null);
                        }}
                      >
                        Use for {action.label.toLowerCase()}
                      </Button>
                    </div>
                  )}
                </SettingsRow>
              );
            })}
          </SettingsGroup>
        );
      })}
      <div className="nt-settings-actions">
        <Button
          variant="secondary"
          icon={RotateCcw}
          iconMotion="undo"
          disabled={Object.keys(overrides).length === 0}
          onClick={() => save(resetBinding(overrides))}
        >
          Reset all shortcuts
        </Button>
      </div>
    </SettingsPage>
  );
}
