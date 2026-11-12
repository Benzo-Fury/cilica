/**
 * The single keybind registry. It drives the key handlers, the command palette labels, the ⌘/
 * cheat sheet and the rebind UI in Settings › Keybinds.
 *
 * Accelerators are strings like "Mod+Shift+R": `Mod` is ⌘ on macOS and Ctrl elsewhere. The key
 * is the physical key (from `KeyboardEvent.code`), so shortcuts work on every keyboard layout.
 * Overrides live in `settings.keybinds` (action id → accelerator; "" unbinds an action).
 */

export type KeybindScope =
  /** Registered by the backend as a global hotkey; works while Margin is in the background. */
  | "global"
  /** Anywhere in the window. */
  | "app"
  /** Only while a note is open. */
  | "note"
  /** Handled by the editor itself; listed for the cheat sheet and rebinding only. */
  | "editor";

export type KeybindGroup = "General" | "Notes" | "View" | "Recording" | "Editing";

export interface ActionDef {
  id: string;
  label: string;
  group: KeybindGroup;
  defaultKeys: string | null;
  scope: KeybindScope;
  /** Fire even when focus is in a text field or the editor. */
  allowInInputs?: boolean;
  /** Can be changed in Settings › Keybinds. */
  rebindable?: boolean;
  /** Offered as a command in the palette. */
  inPalette?: boolean;
}

export const ACTIONS = [
  {
    id: "palette.open",
    label: "Open command palette",
    group: "General",
    defaultKeys: "Mod+K",
    scope: "app",
    allowInInputs: true,
  },
  {
    id: "note.new",
    label: "New note",
    group: "Notes",
    defaultKeys: "Mod+N",
    scope: "app",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "category.new",
    label: "New category",
    group: "Notes",
    defaultKeys: "Mod+Shift+N",
    scope: "app",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "note.find",
    label: "Find in note",
    group: "Notes",
    defaultKeys: "Mod+F",
    scope: "note",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "note.togglePanel",
    label: "Swap Notes and Transcript",
    group: "Notes",
    defaultKeys: "Mod+E",
    scope: "note",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "note.history",
    label: "Show note history",
    group: "Notes",
    defaultKeys: null,
    scope: "note",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "note.trash",
    label: "Move note to Trash",
    group: "Notes",
    defaultKeys: "Mod+Backspace",
    scope: "note",
    inPalette: true,
  },
  {
    id: "recording.toggle",
    label: "Start or stop recording",
    group: "Recording",
    defaultKeys: "Mod+Shift+R",
    scope: "global",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "assistant.toggle",
    label: "Toggle assistant",
    group: "View",
    defaultKeys: "Mod+J",
    scope: "app",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "sidebar.toggle",
    label: "Toggle sidebar",
    group: "View",
    defaultKeys: "Mod+\\",
    scope: "app",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "settings.open",
    label: "Open settings",
    group: "General",
    defaultKeys: "Mod+,",
    scope: "app",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "cheatsheet.open",
    label: "Show keyboard shortcuts",
    group: "General",
    defaultKeys: "Mod+/",
    scope: "app",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "trash.open",
    label: "Open Trash",
    group: "General",
    defaultKeys: null,
    scope: "app",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "assistant.newThread",
    label: "New conversation with Margin",
    group: "View",
    defaultKeys: null,
    scope: "app",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "graph.open",
    label: "Open graph",
    group: "General",
    defaultKeys: "Mod+G",
    scope: "app",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "people.open",
    label: "Open Saved people",
    group: "General",
    defaultKeys: null,
    scope: "app",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "tasks.open",
    label: "Open Scheduled tasks",
    group: "General",
    defaultKeys: null,
    scope: "app",
    allowInInputs: true,
    inPalette: true,
  },
  {
    id: "activity.open",
    label: "Open Margin activity",
    group: "General",
    defaultKeys: null,
    scope: "app",
    allowInInputs: true,
    inPalette: true,
  },
  { id: "edit.undo", label: "Undo", group: "Editing", defaultKeys: "Mod+Z", scope: "editor" },
  { id: "edit.redo", label: "Redo", group: "Editing", defaultKeys: "Mod+Shift+Z", scope: "editor" },
  {
    id: "layer.close",
    label: "Close the topmost layer",
    group: "General",
    defaultKeys: "Escape",
    scope: "app",
    allowInInputs: true,
    rebindable: false,
  },
] as const satisfies readonly ActionDef[];

export type ActionId = (typeof ACTIONS)[number]["id"];

const BY_ID = new Map<string, ActionDef>(ACTIONS.map((a) => [a.id, a]));

export function getAction(id: string): ActionDef | undefined {
  return BY_ID.get(id);
}

export const isRebindable = (action: ActionDef) => action.rebindable !== false;

export type Overrides = Readonly<Record<string, string>>;

// ───────── Accelerators ─────────

const MODIFIERS = ["Mod", "Ctrl", "Alt", "Shift", "Meta"] as const;
type Modifier = (typeof MODIFIERS)[number];

/** `KeyboardEvent.code` → the key name used in accelerators. */
const CODE_KEYS: Record<string, string> = {
  Backspace: "Backspace",
  Delete: "Delete",
  Enter: "Enter",
  NumpadEnter: "Enter",
  Escape: "Escape",
  Tab: "Tab",
  Space: "Space",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  BracketLeft: "[",
  BracketRight: "]",
  Minus: "-",
  Equal: "=",
  Backquote: "`",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
};

/** The accelerator key for a keyboard event, or null for a bare modifier press. */
export function keyFromEvent(event: Pick<KeyboardEvent, "code" | "key">): string | null {
  const { code } = event;
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  const mapped = CODE_KEYS[code];
  if (mapped) return mapped;
  // Unknown code (some virtual keyboards): fall back to the produced character.
  if (/^(Shift|Control|Alt|Meta|OS)/.test(code) || /^(Shift|Control|Alt|Meta)$/.test(event.key)) {
    return null;
  }
  return event.key.length === 1 ? event.key.toUpperCase() : event.key || null;
}

export interface ParsedAccelerator {
  modifiers: ReadonlySet<Modifier>;
  key: string;
}

/** Parse "Mod+Shift+R". Accepts any case and "Cmd"/"Command"/"CmdOrCtrl"/"Control"/"Option". */
export function parseAccelerator(accelerator: string): ParsedAccelerator | null {
  if (!accelerator) return null;
  // "Mod++" style is not used; split on "+" that is followed by something.
  const parts = accelerator.split(/\+(?=.)/).map((p) => p.trim());
  const key = parts.pop();
  if (!key) return null;
  const modifiers = new Set<Modifier>();
  for (const raw of parts) {
    const p = raw.toLowerCase();
    if (["mod", "cmdorctrl", "commandorcontrol"].includes(p)) modifiers.add("Mod");
    else if (["ctrl", "control"].includes(p)) modifiers.add("Ctrl");
    else if (["alt", "option", "opt"].includes(p)) modifiers.add("Alt");
    else if (p === "shift") modifiers.add("Shift");
    else if (["meta", "cmd", "command", "super", "win"].includes(p)) modifiers.add("Meta");
    else return null;
  }
  const normalisedKey = key.length === 1 ? key.toUpperCase() : normaliseNamedKey(key);
  return { modifiers, key: normalisedKey };
}

function normaliseNamedKey(key: string): string {
  const lower = key.toLowerCase();
  const named: Record<string, string> = {
    esc: "Escape",
    escape: "Escape",
    return: "Enter",
    enter: "Enter",
    backspace: "Backspace",
    delete: "Delete",
    del: "Delete",
    up: "Up",
    down: "Down",
    left: "Left",
    right: "Right",
    arrowup: "Up",
    arrowdown: "Down",
    arrowleft: "Left",
    arrowright: "Right",
    space: "Space",
    tab: "Tab",
    comma: ",",
    slash: "/",
    backslash: "\\",
    period: ".",
  };
  return named[lower] ?? (/^f\d{1,2}$/.test(lower) ? lower.toUpperCase() : key);
}

/** Canonical form: modifiers in a fixed order (Mod, Ctrl, Alt, Shift, Meta) then the key. */
export function normalizeAccelerator(accelerator: string): string | null {
  const parsed = parseAccelerator(accelerator);
  if (!parsed) return null;
  return [...MODIFIERS.filter((m) => parsed.modifiers.has(m)), parsed.key].join("+");
}

/** The canonical accelerator a keyboard event produces on this platform. */
export function eventToAccelerator(
  event: Pick<KeyboardEvent, "code" | "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">,
  isMac: boolean,
): string | null {
  const key = keyFromEvent(event);
  if (!key) return null;
  const mods: Modifier[] = [];
  if (isMac ? event.metaKey : event.ctrlKey) mods.push("Mod");
  if (isMac && event.ctrlKey) mods.push("Ctrl");
  if (event.altKey) mods.push("Alt");
  if (event.shiftKey) mods.push("Shift");
  if (!isMac && event.metaKey) mods.push("Meta");
  return [...mods, key].join("+");
}

/**
 * A shortcut people can bind: needs Mod, Ctrl or Alt (so typing never triggers it),
 * unless it is a function key.
 */
export function isValidBinding(accelerator: string): boolean {
  const parsed = parseAccelerator(accelerator);
  if (!parsed) return false;
  if (/^F\d{1,2}$/.test(parsed.key)) return true;
  if (parsed.key === "Escape") return false;
  return parsed.modifiers.has("Mod") || parsed.modifiers.has("Ctrl") || parsed.modifiers.has("Alt");
}

// ───────── Bindings ─────────

/** The accelerator an action is bound to after overrides ("" or null when unbound). */
export function effectiveKeys(action: ActionDef, overrides: Overrides): string | null {
  if (isRebindable(action) && Object.hasOwn(overrides, action.id)) {
    const value = overrides[action.id];
    return value ? normalizeAccelerator(value) : null;
  }
  return action.defaultKeys ? normalizeAccelerator(action.defaultKeys) : null;
}

/** Every action with its effective accelerator. */
export function resolveBindings(
  overrides: Overrides,
  actions: readonly ActionDef[] = ACTIONS,
): Map<string, string | null> {
  return new Map(actions.map((a) => [a.id, effectiveKeys(a, overrides)]));
}

/** The action bound to a canonical accelerator, if any (first match in registry order). */
export function matchAction(
  accelerator: string,
  overrides: Overrides,
  actions: readonly ActionDef[] = ACTIONS,
): ActionDef | undefined {
  return actions.find((a) => effectiveKeys(a, overrides) === accelerator);
}

/** Another action already using `accelerator` (ignoring `actionId` itself). */
export function findConflict(
  actionId: string,
  accelerator: string,
  overrides: Overrides,
  actions: readonly ActionDef[] = ACTIONS,
): ActionDef | undefined {
  const target = normalizeAccelerator(accelerator);
  if (!target) return undefined;
  return actions.find((a) => a.id !== actionId && effectiveKeys(a, overrides) === target);
}

/** Groups of action ids that share one accelerator. */
export function listConflicts(
  overrides: Overrides,
  actions: readonly ActionDef[] = ACTIONS,
): string[][] {
  const byKeys = new Map<string, string[]>();
  for (const action of actions) {
    const keys = effectiveKeys(action, overrides);
    if (!keys) continue;
    byKeys.set(keys, [...(byKeys.get(keys) ?? []), action.id]);
  }
  return [...byKeys.values()].filter((ids) => ids.length > 1);
}

/**
 * New overrides after binding `actionId` to `accelerator`. With `replace`, the action that held
 * the shortcut is unbound; binding back to the default removes the override.
 */
export function rebind(
  overrides: Overrides,
  actionId: string,
  accelerator: string | null,
  options: { replace?: boolean; actions?: readonly ActionDef[] } = {},
): Record<string, string> {
  const actions = options.actions ?? ACTIONS;
  const next: Record<string, string> = { ...overrides };
  const action = actions.find((a) => a.id === actionId);
  if (!action) return next;
  const normalised = accelerator ? normalizeAccelerator(accelerator) : null;
  if (options.replace && normalised) {
    const other = findConflict(actionId, normalised, overrides, actions);
    if (other) next[other.id] = "";
  }
  const def = action.defaultKeys ? normalizeAccelerator(action.defaultKeys) : null;
  if (normalised === def) delete next[actionId];
  else next[actionId] = normalised ?? "";
  return next;
}

/** Remove the override for one action, or every override. */
export function resetBinding(overrides: Overrides, actionId?: string): Record<string, string> {
  if (!actionId) return {};
  const next: Record<string, string> = { ...overrides };
  delete next[actionId];
  return next;
}

// ───────── Display ─────────

const MAC_GLYPHS: Record<string, string> = {
  Mod: "⌘",
  Ctrl: "⌃",
  Alt: "⌥",
  Shift: "⇧",
  Meta: "⌘",
  Backspace: "⌫",
  Delete: "⌦",
  Enter: "↩",
  Escape: "esc",
  Tab: "⇥",
  Space: "Space",
  Up: "↑",
  Down: "↓",
  Left: "←",
  Right: "→",
};

const PC_NAMES: Record<string, string> = {
  Mod: "Ctrl",
  Ctrl: "Ctrl",
  Alt: "Alt",
  Shift: "Shift",
  Meta: "Win",
  Backspace: "Backspace",
  Delete: "Del",
  Enter: "Enter",
  Escape: "Esc",
  Up: "↑",
  Down: "↓",
  Left: "←",
  Right: "→",
};

/** Keycaps for display: ⌘ ⇧ R on macOS, Ctrl Shift R elsewhere. */
export function formatAccelerator(accelerator: string | null, isMac: boolean): string[] {
  if (!accelerator) return [];
  const parsed = parseAccelerator(accelerator);
  if (!parsed) return [];
  const names = isMac ? MAC_GLYPHS : PC_NAMES;
  const mods = MODIFIERS.filter((m) => parsed.modifiers.has(m)).map((m) => names[m] ?? m);
  return [...mods, names[parsed.key] ?? parsed.key];
}

/** One string for aria labels and tooltips' accessible text: "⌘⇧R" or "Ctrl+Shift+R". */
export function acceleratorLabel(accelerator: string | null, isMac: boolean): string {
  return formatAccelerator(accelerator, isMac).join(isMac ? "" : "+");
}
