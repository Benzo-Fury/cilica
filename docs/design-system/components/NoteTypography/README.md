# NoteTypography

The note page is the content layer: opaque `bg-canvas`, a 720px measure, generous padding and the note-body styles.

**Order**: title (`display`, 28/34, opsz 32), a row of meta chips (date, duration, people, category), then the Markdown body.

**Body styles**: `note-h1` / `note-h2` / `note-h3` for headings, `note-body` 15/24 for paragraphs and lists, tasks with Checkbox, quotes indented 16px behind a 2px `line-strong` rule, inline `code` in Geist Mono on `bg-sunken`. Mentions of people are pills with a 16px avatar.

**Note font** (Settings › Appearance): Sans (default), Serif (Newsreader 17/27) or Mono (Geist Mono 14/22). Headings stay in Sans.

**AI-written text** arriving into a note fades in word by word (`--dur-slow`) and keeps an `accent-soft` wash for 2s before settling.

Leave `space-16` of runway under the last line so the recorder capsule never covers it.
