/**
 * Prose diffs over Markdown, for AI edit cards and note history.
 *
 * Both documents are split into blocks (paragraphs, headings, list items…). Blocks that differ
 * form hunks; inside a hunk, words are diffed on the readable text, so a card shows the whole
 * sentence around a change in the note's own typography rather than raw Markdown.
 * `finalMarkdown` rebuilds the body with only the accepted hunks applied (`applyProposal`'s
 * `final_md`).
 */
import { diffArrays, diffWords } from "diff";
import { type Block, type BlockKind, blockProse, splitBlocks } from "./markdown";

export interface DiffPart {
  value: string;
  added?: boolean;
  removed?: boolean;
}

export interface DiffRow {
  kind: BlockKind;
  level?: number;
  checked?: boolean;
  number?: number;
  depth?: number;
  change: "same" | "changed" | "added" | "removed";
  parts: DiffPart[];
}

export interface Hunk {
  /** 0-based, in document order. */
  index: number;
  before: Block[];
  after: Block[];
  /** The nearest heading above the change, for the card header ("Action items"). */
  section: string | null;
  rows: DiffRow[];
}

type Op = { type: "same"; before: Block[]; after: Block[] } | { type: "hunk"; hunk: Hunk };

export interface ProseDiff {
  ops: Op[];
  hunks: Hunk[];
  before: string;
  after: string;
}

const key = (block: Block) => block.raw.replace(/\s+$/, "");

function rowOf(block: Block, change: DiffRow["change"], parts: DiffPart[]): DiffRow {
  return {
    kind: block.kind,
    level: block.level,
    checked: block.checked,
    number: block.number,
    depth: block.depth,
    change,
    parts,
  };
}

/** Rows for one changed region: paired blocks get a word diff, the rest are whole inserts/strikes. */
function hunkRows(before: Block[], after: Block[]): DiffRow[] {
  if (before.length === after.length) {
    return after.map((a, i) => {
      const b = before[i] as Block;
      // When only a marker changed (a task ticked) the words match and the row shows the new state.
      return rowOf(a, "changed", diffWords(blockProse(b), blockProse(a)) as DiffPart[]);
    });
  }
  return [
    ...before.map((b) => rowOf(b, "removed", [{ value: blockProse(b), removed: true }])),
    ...after.map((a) => rowOf(a, "added", [{ value: blockProse(a), added: true }])),
  ];
}

/** Diff two Markdown bodies into same-blocks and hunks. */
export function proseDiff(beforeMd: string, afterMd: string): ProseDiff {
  const before = splitBlocks(beforeMd);
  const after = splitBlocks(afterMd);
  const changes = diffArrays(before.map(key), after.map(key));
  const ops: Op[] = [];
  const hunks: Hunk[] = [];
  let bi = 0;
  let ai = 0;
  let section: string | null = null;
  let pending: { before: Block[]; after: Block[] } | null = null;
  const lastHeading = (blocks: Block[]) => {
    for (let i = blocks.length - 1; i >= 0; i--) {
      const b = blocks[i];
      if (b?.kind === "heading") return blockProse(b);
    }
    return null;
  };
  const flush = () => {
    if (!pending) return;
    const hunk: Hunk = {
      index: hunks.length,
      before: pending.before,
      after: pending.after,
      section: lastHeading(pending.after.slice(0, 1)) ?? section,
      rows: hunkRows(pending.before, pending.after),
    };
    hunks.push(hunk);
    ops.push({ type: "hunk", hunk });
    section = lastHeading(pending.after) ?? section;
    pending = null;
  };
  for (const change of changes) {
    const count = change.count ?? change.value.length;
    if (change.removed) {
      pending ??= { before: [], after: [] };
      pending.before.push(...before.slice(bi, bi + count));
      bi += count;
    } else if (change.added) {
      pending ??= { before: [], after: [] };
      pending.after.push(...after.slice(ai, ai + count));
      ai += count;
    } else {
      flush();
      const same = {
        type: "same" as const,
        before: before.slice(bi, bi + count),
        after: after.slice(ai, ai + count),
      };
      ops.push(same);
      section = lastHeading(same.after) ?? section;
      bi += count;
      ai += count;
    }
  }
  flush();
  return { ops, hunks, before: beforeMd, after: afterMd };
}

const LIST: ReadonlySet<BlockKind> = new Set(["bullet", "ordered", "task"]);

/**
 * The body with only `accepted` hunks applied. Accepting every hunk gives back `after` exactly;
 * where a kept block meets a changed one, a blank line is restored so blocks never run together.
 */
export function finalMarkdown(diff: ProseDiff, accepted: ReadonlySet<number>): string {
  let out = "";
  let lastKind: BlockKind | null = null;
  let lastSide: "before" | "after" | null = null;
  const append = (blocks: Block[], side: "before" | "after") => {
    for (const block of blocks) {
      if (out && lastSide && lastSide !== side) {
        if (!out.endsWith("\n")) out += "\n";
        const bothList = lastKind && LIST.has(lastKind) && LIST.has(block.kind);
        if (!bothList && !out.endsWith("\n\n")) out += "\n";
      }
      out += block.raw;
      lastKind = block.kind;
      lastSide = side;
    }
  };
  for (const op of diff.ops) {
    if (op.type === "same") append(op.after, "after");
    else if (accepted.has(op.hunk.index)) append(op.hunk.after, "after");
    else append(op.hunk.before, "before");
  }
  return out;
}

/** The whole document as rows (unchanged blocks plain, changes marked), for history. */
export function documentRows(diff: ProseDiff): DiffRow[] {
  return diff.ops.flatMap((op) =>
    op.type === "same"
      ? op.after.map((b) => rowOf(b, "same", [{ value: blockProse(b) }]))
      : op.hunk.rows,
  );
}

/** Words added and removed across a diff. */
export function diffStats(rows: readonly DiffRow[]): { added: number; removed: number } {
  const words = (s: string) => s.trim().split(/\s+/).filter(Boolean).length;
  let added = 0;
  let removed = 0;
  for (const row of rows) {
    for (const p of row.parts) {
      if (p.added) added += words(p.value);
      if (p.removed) removed += words(p.value);
    }
  }
  return { added, removed };
}
