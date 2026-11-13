# References

What Cilica borrows from the products it was studied against, so later decisions stay consistent with the reasons.

| Pattern | Seen in | Cilica's version |
| --- | --- | --- |
| A small floating pill that shows recording with live bars | Granola ("Transcribing" pill), Notion AI meeting notes (floating mini card), Loom (dark control bar) | Recorder capsule in glass, plus the Dynamic Island when Cilica is in the background |
| A big black Record capsule bottom-centre | Voicenotes | Idle capsule bottom-centre, but glass and quiet, because it sits over the note |
| Speaker identity as a coloured initial, text stays monochrome | Amie, Fireflies, Grain | Colour only on the avatar ring; Saved People choose theirs |
| Inset content sheet on a grey window, panels around it | Linear (2025–26), macOS Tahoe | Not used: the shell is one flush sheet split by hairlines |
| User turns as bubbles, assistant as plain text, tool steps collapsed to one line | Searchable, Mintlify, Perplexity, Linear Ask | Assistant thread |
| Prose diff with inline insert and strike | Linear version history, Semrush review mode | Diff card with `accent-soft` inserts and `danger` strikes |
| Palette with grouped results, a sliding highlight and key hints in a footer | Linear, Plane, incident.io, Mercury, Mintlify | Command palette (⌘K) on strong glass over a scrim |
| Settings as grouped rows with a description under each label | Linear, Gumloop, Plain | Settings pages using `bg-raised` groups |
| Permission rows with an icon, a sentence and one action | Tana, Maze, Loom | PermissionRow |
| System font and shadcn stone palette; squircles via CSS `corner-shape` | Anarlog (from its source) | A bundled Inter instead of system-ui, a custom warm-graphite palette, squircles with a WebKit fallback |
| Completion sound that people can choose | Anarlog (`cuelume`: ready, chime, bloom…) | Notes-ready chime, choosable |
| Serif note titles and a warm paper surface | Granola | Optional Serif note font; titles stay in Inter |

Sources: Mobbin web screens for Granola-adjacent tools (Voicenotes, Notion, Amie, Fireflies, Otter, Grain, Lightfield, Tana, Linear, Plane, incident.io, Mercury, Mintlify, Searchable, Semrush, Loom, Maze), granola.ai and anarlog.so product pages, the Anarlog repository's `apps/desktop/src/styles`, Apple's Liquid Glass materials, Material Design's dark theme guidance (#121212 base, off-white text), `window-vibrancy` 0.7 docs (Mica, vibrancy, `apply_liquid_glass`), and current CSS `corner-shape` support (Chromium 139+ only).
