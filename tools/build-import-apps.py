#!/usr/bin/env python3
"""Generate docs/images/import-apps.svg, the README's grid of apps Margin imports from.
Reads the catalogue in apps/desktop/src/features/import/providers.ts and the icons in its logos/
folder, and embeds each icon so the image loads nothing from elsewhere.
Run: bun run import-apps   (import-apps.svg is generated; never edit it by hand)"""
import base64, html, pathlib, re

root = pathlib.Path(__file__).resolve().parent.parent
feature = root / "apps/desktop/src/features/import"
out = root / "docs/images/import-apps.svg"

# ── The catalogue, in the order Settings › Import lists it ──────────────────
ts = (feature / "providers.ts").read_text()
entries = re.findall(r'\n    id: "([a-z_]+)",\n    name: "([^"]+)",\n    group: "([a-z]+)"', ts)
GROUPS = [("notes", "Notes apps"), ("meetings", "Meeting note takers"), ("platforms", "Meeting apps")]
SAME_LOGO = {"notion_meetings": "notion"}

# ── Layout ──────────────────────────────────────────────────────────────────
COLS, CELL_W, CELL_H, ICON, PAD, LABEL_CHARS = 9, 100, 104, 52, 36, 13
WIDTH = PAD * 2 + COLS * CELL_W
FONT = '-apple-system,BlinkMacSystemFont,"Segoe UI",Helvetica,Arial,sans-serif'


def icon_data(provider_id: str) -> str:
    """The app's icon as a data URI. Each file is the whole mark, its own shape included."""
    name = SAME_LOGO.get(provider_id, provider_id)
    for ext, mime in (("png", "image/png"), ("svg", "image/svg+xml")):
        path = feature / "logos" / f"{name}.{ext}"
        if path.exists():
            return f"data:{mime};base64,{base64.b64encode(path.read_bytes()).decode()}"
    raise SystemExit(f"No icon for {provider_id} in {feature / 'logos'}")


def label_lines(name: str) -> list[str]:
    """A name on one line, or two when it is long."""
    if len(name) <= LABEL_CHARS:
        return [name]
    words, first = name.split(), ""
    for i, word in enumerate(words):
        if first and len(f"{first} {word}") > LABEL_CHARS:
            return [first, " ".join(words[i:])]
        first = f"{first} {word}".strip()
    return [first]


# ── Drawing ─────────────────────────────────────────────────────────────────
parts, y = [], PAD
for group, heading in GROUPS:
    items = [(pid, name) for pid, name, g in entries if g == group]
    parts.append(f'<text x="{PAD + 14}" y="{y + 14}" class="h">{html.escape(heading.upper())} · {len(items)}</text>')
    y += 32
    for k, (pid, name) in enumerate(items):
        cx = PAD + (k % COLS) * CELL_W + CELL_W / 2
        top = y + (k // COLS) * CELL_H
        parts.append(f'<image href="{icon_data(pid)}" x="{cx - ICON / 2}" y="{top}" width="{ICON}" height="{ICON}"/>')
        for j, line in enumerate(label_lines(name)):
            parts.append(f'<text x="{cx}" y="{top + ICON + 18 + j * 15}" class="n">{html.escape(line)}</text>')
    y += -(-len(items) // COLS) * CELL_H + 18
height = y + PAD - 18

names = ", ".join(name for _, name, g in entries if g != "other")
svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}" role="img" aria-label="Apps Margin imports from">
<title>Apps Margin imports from: {html.escape(names)}</title>
<style>
.h{{font:600 12px/1 {FONT};letter-spacing:.06em;fill:#8f8a84}}
.n{{font:500 12.5px/1 {FONT};fill:#e9e5df;text-anchor:middle}}
</style>
<rect width="{WIDTH}" height="{height}" rx="20" fill="#1c1b1a"/>
{"".join(parts)}
</svg>
'''
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(svg)
print(f"Wrote {out.relative_to(root)}: {len(entries) - 1} apps, {WIDTH}×{height}, {len(svg) // 1024} KB")
