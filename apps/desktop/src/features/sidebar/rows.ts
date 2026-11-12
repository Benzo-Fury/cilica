/** The layout id of the sidebar's one sliding selection pill. */
export const SIDEBAR_PILL = "sidebar-pill";

/** Moves focus between sidebar rows with ↑/↓. */
export function focusSibling(from: HTMLElement, direction: -1 | 1) {
  const rows = Array.from(
    from.closest("[data-sidebar]")?.querySelectorAll<HTMLElement>("[data-nav-row]") ?? [],
  );
  rows[rows.indexOf(from) + direction]?.focus();
}
