# AssistantThread

The assistant thread shows the conversation in the right panel.

- **Your turns** are `fill-press` bubbles, right-aligned, max 85% width.
- **Margin's replies** are plain text on the panel, no bubble, no avatar. Citations link to the note and timecode they came from.
- **Tool steps** ("Searched 3 notes", "Read Q3 planning sync") are single `ink-3` lines that expand to show detail.
- **Working**: the current step label shimmers (a light band sweeps across the text, 1.8s linear). This is the only "AI glow" in the product.

New replies stream in word by word with `nt-word-in`; the thread auto-scrolls until you scroll up.

## While Margin works

- A running step names what it is touching ("Reading “Q3 planning sync”"); between the last step and the first words the thread says "Writing it down…".
- A soft halo (`nt-margin-glow`) breathes around the composer. It never sits behind thread text, and is off under Reduce transparency.
- Citations with a time render as a receipt: the title opens the note, the time chip opens the transcript at that moment. The chip only appears when the note still has that part of its transcript.
