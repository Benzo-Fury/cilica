/** ⌘/: every shortcut, generated from the keybind registry (so rebinds show up here). */
import { Keyboard } from "lucide-react";
import {
  ACTIONS,
  effectiveKeys,
  formatAccelerator,
  type KeybindGroup,
  useKeybindStore,
} from "@/app/keybinds";
import { isMac } from "@/app/platform";
import { openSettings } from "@/app/router";
import { useOverlays } from "@/features/shell/overlays";
import { Button, DialogRoot, DialogSurface, DialogTitle, Kbd } from "@/ui";

const GROUPS: KeybindGroup[] = ["General", "Notes", "View", "Recording", "Editing"];

export function CheatSheet() {
  const open = useOverlays((s) => s.open === "cheatsheet");
  const close = useOverlays((s) => s.close);
  const overrides = useKeybindStore((s) => s.overrides);
  return (
    <DialogRoot open={open} onOpenChange={(o) => !o && close()}>
      {open && (
        <DialogSurface
          variant="glass"
          placement="top"
          className="nt-cheatsheet"
          description="Keyboard shortcuts"
        >
          <div className="nt-cheats-head">
            <DialogTitle>Keyboard shortcuts</DialogTitle>
            <Button
              size="sm"
              variant="ghost"
              icon={Keyboard}
              onClick={() => {
                close();
                openSettings("keybinds");
              }}
            >
              Change shortcuts
            </Button>
          </div>
          <div className="nt-cheats">
            {GROUPS.map((group) => {
              const actions = ACTIONS.filter(
                (a) => a.group === group && effectiveKeys(a, overrides),
              );
              if (actions.length === 0) return null;
              return (
                <section key={group} className="nt-cheat-group">
                  <h3>{group}</h3>
                  {actions.map((a) => (
                    <div key={a.id} className="nt-cheat-row">
                      <span>{a.label}</span>
                      <Kbd keys={formatAccelerator(effectiveKeys(a, overrides), isMac)} />
                    </div>
                  ))}
                </section>
              );
            })}
          </div>
        </DialogSurface>
      )}
    </DialogRoot>
  );
}
