/**
 * Geometry for the tab bar's ground. The page and the selected tab are one shape: the page's
 * bottom edge turns down into the tab through two concave fillets, and the page's own bottom
 * corners round off into the bar. The bar paints its ground everywhere outside that shape.
 *
 * Coordinates: y = 0 is `strip` px above the page's bottom edge (the SVG overlaps the page by that
 * much to draw its corners); the page's bottom edge is y = strip; the bar runs below it.
 *
 * Every radius shrinks to fit the room it has, so the shape stays valid at any point of an
 * animation: a tab flush with the left edge has no fillet and no page corner on that side, and a
 * tab whose depth is 0 has melted back into the page.
 */

export interface TabShape {
  /** Width of the bar, px. */
  width: number;
  /** Selected tab's left and right edges, px from the bar's left. */
  left: number;
  right: number;
  /** How far the selected tab reaches below the page's edge, px. 0 for no selected tab. */
  depth: number;
}

export interface TabShapeRadii {
  /** Height of the band above the page's edge the SVG covers; also the page's corner radius. */
  strip: number;
  /** Concave fillet where the page turns down into the tab. */
  fillet: number;
  /** The tab's own bottom corners. */
  corner: number;
}

export const TAB_RADII: TabShapeRadii = { strip: 12, fillet: 10, corner: 10 };

const n = (v: number) => Math.round(v * 100) / 100;
const arc = (r: number, sweep: 0 | 1, x: number, y: number) =>
  r > 0.01 ? `A${n(r)} ${n(r)} 0 0 ${sweep} ${n(x)} ${n(y)}` : `L${n(x)} ${n(y)}`;

/**
 * The page-and-tab outline as an open path: down the page's left edge, round the tab, and up the
 * right edge. Used for the hairline, and closed along the top for the cut-out.
 */
export function tabOutline(shape: TabShape, radii: TabShapeRadii = TAB_RADII): string {
  const { width: w } = shape;
  const s = radii.strip;
  const left = Math.max(0, Math.min(shape.left, w));
  const right = Math.max(left, Math.min(shape.right, w));
  const tabWidth = right - left;
  const depth = tabWidth > 0.5 ? Math.max(0, shape.depth) : 0;

  if (depth < 0.5) {
    const r = Math.min(s, w / 2);
    return [
      `M0 0`,
      `L0 ${n(s - r)}`,
      arc(r, 0, r, s),
      `L${n(w - r)} ${n(s)}`,
      arc(r, 0, w, s - r),
      `L${n(w)} 0`,
    ].join(" ");
  }

  const roomLeft = left;
  const roomRight = w - right;
  const filletLeft = Math.min(radii.fillet, roomLeft / 2, depth);
  const filletRight = Math.min(radii.fillet, roomRight / 2, depth);
  const cornerLeft = Math.max(0, Math.min(s, roomLeft - filletLeft));
  const cornerRight = Math.max(0, Math.min(s, roomRight - filletRight));
  const tabCorner = Math.min(radii.corner, tabWidth / 2, depth / 2);
  const bottom = s + depth;

  return [
    `M0 0`,
    `L0 ${n(s - cornerLeft)}`,
    arc(cornerLeft, 0, cornerLeft, s),
    `L${n(left - filletLeft)} ${n(s)}`,
    arc(filletLeft, 1, left, s + filletLeft),
    `L${n(left)} ${n(bottom - tabCorner)}`,
    arc(tabCorner, 0, left + tabCorner, bottom),
    `L${n(right - tabCorner)} ${n(bottom)}`,
    arc(tabCorner, 0, right, bottom - tabCorner),
    `L${n(right)} ${n(s + filletRight)}`,
    arc(filletRight, 1, right + filletRight, s),
    `L${n(w - cornerRight)} ${n(s)}`,
    arc(cornerRight, 0, w, s - cornerRight),
    `L${n(w)} 0`,
  ].join(" ");
}

/**
 * The bar's ground: its whole area with the page-and-tab shape cut out (even-odd fill).
 * `height` is the bar's height below the page's edge.
 */
export function tabGround(
  shape: TabShape,
  height: number,
  radii: TabShapeRadii = TAB_RADII,
): string {
  const w = n(shape.width);
  const h = n(radii.strip + height);
  return `M0 0 L${w} 0 L${w} ${h} L0 ${h} Z ${tabOutline(shape, radii)} Z`;
}
