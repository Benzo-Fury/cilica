# Field

Fields take short text: search, names, API keys, rename.

**Consumer provides**: an `id`d input, a placeholder that is an example rather than an instruction ("Search notes and transcripts"), optional leading icon and trailing Kbd.

**States**: rest `bg-sunken` with a 0.5px `line-strong` edge; hover thickens the edge; focus switches to `bg-raised`, a 1px `accent-text` edge and a 3px `accent-soft` halo that fades in over `--dur-base`; error swaps both for `danger` / `danger-soft` and shakes once (320ms).

**Errors** sit under the field in `callout` `ink-3` and say what to do ("That key was rejected by Anthropic. Check it and paste it again."). Never clear what the person typed.

**Secrets** (API keys) are masked after save and stored in the OS keychain; show the last 4 characters only.
