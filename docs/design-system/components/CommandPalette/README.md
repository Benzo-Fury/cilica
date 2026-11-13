# CommandPalette

The command palette (⌘K) finds notes, people and commands from anywhere.

**Layout**: 640px wide, `radius-xl`, `glass-fill-strong` over a `scrim` with `blur-scrim`. A 52px input row, a row of filter chips when the search understood any, grouped results (Filters, Notes, Transcript matches, Conversations, Tasks, In Trash, Commands, Settings, Ask), a footer with key hints.

**Filter chips** (`PaletteFilters`, `.nt-palette-filters`): neutral `ChipButton`s with one icon for the kind of filter (calendar, clock, person, folder, hash…), `space-1-5` apart, `space-2` × `space-4` padding, a 0.5px `line` under the row. Clicking a chip removes the filter; focus stays in the input. A quiet `ink-3` text action ("Search the words instead") sits at the end of the row.

**Rows**: 36px, `radius-sm` (concentric inside the 12px list padding). The highlight is one element that slides between rows in 140ms ease-out (no spring: it moves dozens of times a minute). Matched characters are bold, not coloured.

**Open**: scale 0.96 → 1 and 6px drop with `--spring-smooth`; scrim fades in `--dur-base`. **Close**: fade and scale to 0.98 in `--dur-fast`. Results update without animating height.

**⌘↩** sends the query to the assistant instead.
