/**
 * ⌘Z outside the editor undoes the last thing Margin did. The backend raises one Undo toast per
 * assistant turn (`undo_turn`); accepting a proposal raises one for its action. While that toast
 * is on screen ⌘Z presses its button; after it has gone, ⌘Z still reaches the same undo here.
 */
type LastUndo = { label: string; run: () => Promise<unknown> | undefined };

let last: LastUndo | null = null;

/** Remember the newest undoable AI change (replaces the previous one). */
export function rememberAiUndo(label: string, run: () => Promise<unknown> | undefined) {
  last = { label, run };
}

/** Forget it once it ran (from its toast or ⌘Z) so ⌘Z never undoes twice. */
export function forgetAiUndo(run?: LastUndo["run"]) {
  if (!run || last?.run === run) last = null;
}

/** Run the remembered undo. False when there is nothing to undo. */
export function undoLastAiChange(): boolean {
  const current = last;
  if (!current) return false;
  last = null;
  void current.run();
  return true;
}

export function hasAiUndo(): boolean {
  return last !== null;
}
