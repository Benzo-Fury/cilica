import { byPosition, impliedPositions, positionBetween, positionForInsert } from "./positions";

const items = (...positions: (number | null)[]) =>
  positions.map((position, i) => ({ id: String.fromCharCode(97 + i), position }));

describe("positionBetween", () => {
  it("takes the midpoint, or steps past an open end", () => {
    expect(positionBetween(1, 2)).toBe(1.5);
    expect(positionBetween(null, 2)).toBe(1);
    expect(positionBetween(3, null)).toBe(4);
    expect(positionBetween(null, null)).toBe(0);
  });

  it("keeps working after many inserts in the same gap", () => {
    let lo = 0;
    const hi = 1;
    for (let i = 0; i < 40; i++) {
      const mid = positionBetween(lo, hi);
      expect(mid).toBeGreaterThan(lo);
      expect(mid).toBeLessThan(hi);
      lo = mid;
    }
  });
});

describe("impliedPositions", () => {
  it("fills missing positions from order", () => {
    expect(impliedPositions(items(1, null, 3))).toEqual([1, 2, 3]);
    expect(impliedPositions(items(null, null))).toEqual([0, 1]);
    const filled = impliedPositions(items(null, 5));
    expect(filled[0]).toBeLessThan(5);
  });

  it("repairs out-of-order positions so the result stays sorted", () => {
    const filled = impliedPositions(items(1, 4, 2, 6));
    for (let i = 1; i < filled.length; i++)
      expect(filled[i]).toBeGreaterThan(filled[i - 1] ?? -Infinity);
  });
});

describe("positionForInsert", () => {
  const list = items(1, 2, 3, 4); // a b c d

  it("moves an item down between two others", () => {
    // a to between c and d (index 3 in the original list)
    expect(positionForInsert(list, "a", 3)).toBe(3.5);
  });

  it("moves an item up to the top", () => {
    expect(positionForInsert(list, "d", 0)).toBe(0);
  });

  it("moves an item to the end", () => {
    expect(positionForInsert(list, "b", 4)).toBe(5);
  });

  it("returns null when the item lands where it already is", () => {
    expect(positionForInsert(list, "b", 1)).toBeNull();
    expect(positionForInsert(list, "b", 2)).toBeNull();
  });

  it("inserts a newcomer from another bucket", () => {
    expect(positionForInsert(list, "z", 2)).toBe(2.5);
    expect(positionForInsert([], "z", 0)).toBe(0);
  });

  it("sorts nulls last", () => {
    expect(byPosition(items(null, 2, 1)).map((i) => i.id)).toEqual(["c", "b", "a"]);
  });
});
