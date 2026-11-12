/**
 * Fractional positions for drag and drop (lower sorts first). Inserting between two items takes
 * the midpoint, so only the moved item is written. Items without a position (legacy rows) get
 * one implied by their order.
 */

export interface Positioned {
  id: string;
  position: number | null;
}

/** A position strictly between `before` and `after` (either may be missing). */
export function positionBetween(before: number | null, after: number | null): number {
  if (before === null && after === null) return 0;
  if (before === null) return (after as number) - 1;
  if (after === null) return before + 1;
  if (after <= before) return before + 1e-6; // degenerate input; keep it sortable
  return (before + after) / 2;
}

/** Positions with gaps filled from list order (the list is already sorted). */
export function impliedPositions(items: readonly Positioned[]): number[] {
  const result: number[] = [];
  let last: number | null = null;
  for (let i = 0; i < items.length; i++) {
    const own = items[i]?.position ?? null;
    if (own !== null && (last === null || own > last)) {
      result.push(own);
      last = own;
      continue;
    }
    // Find the next explicit position to interpolate towards.
    let next: number | null = null;
    for (let j = i + 1; j < items.length; j++) {
      const p = items[j]?.position ?? null;
      if (p !== null && (last === null || p > last)) {
        next = p;
        break;
      }
    }
    const value = positionBetween(last, next);
    result.push(value);
    last = value;
  }
  return result;
}

/**
 * The position for `movingId` inserted at `index` of `list` (the target bucket in display
 * order, which may include the moving item itself). Returns null when the move changes nothing.
 */
export function positionForInsert(
  list: readonly Positioned[],
  movingId: string,
  index: number,
): number | null {
  const currentIndex = list.findIndex((i) => i.id === movingId);
  const positions = impliedPositions(list);
  const others = list
    .map((item, i) => ({ id: item.id, position: positions[i] ?? 0 }))
    .filter((item) => item.id !== movingId);
  // Index in `others`: shift down when the moving item was above the insertion point.
  let target = currentIndex >= 0 && currentIndex < index ? index - 1 : index;
  target = Math.max(0, Math.min(others.length, target));
  if (currentIndex >= 0 && target === currentIndex) return null;
  const before = others[target - 1]?.position ?? null;
  const after = others[target]?.position ?? null;
  return positionBetween(before, after);
}

/** Sort by position (nulls last, stable). */
export function byPosition<T extends Positioned>(items: readonly T[]): T[] {
  return [...items].sort((a, b) => {
    if (a.position === null && b.position === null) return 0;
    if (a.position === null) return 1;
    if (b.position === null) return -1;
    return a.position - b.position;
  });
}
