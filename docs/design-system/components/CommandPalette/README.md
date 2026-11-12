# CommandPalette

The command palette (⌘K) finds notes, people and commands from anywhere.

**Layout**: 640px wide, `radius-xl`, `glass-fill-strong` over a `scrim` with `blur-scrim`. A 52px input row, grouped results (Notes, People, Commands, Ask), a footer with key hints.

**Rows**: 36px, `radius-sm` (concentric inside the 12px list padding). The highlight is one element that slides between rows in 140ms ease-out (no spring: it moves dozens of times a minute). Matched characters are bold, not coloured.

**Open**: scale 0.96 → 1 and 6px drop with `--spring-smooth`; scrim fades in `--dur-base`. **Close**: fade and scale to 0.98 in `--dur-fast`. Results update without animating height.

**⌘↩** sends the query to the assistant instead.
