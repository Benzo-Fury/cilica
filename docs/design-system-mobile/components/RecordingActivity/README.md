# RecordingActivity

Recording continues in the background and on the lock screen. The system surface always shows what is recording, for how long, and how to stop it.

## iOS: Live Activity

| Presentation | Content |
| --- | --- |
| Compact (Dynamic Island) | Leading: the breathing `rec` dot. Trailing: the timer (`Text(timerInterval:)`, monospaced digits) |
| Minimal | The `rec` dot |
| Expanded | Note title · timer · live level waveform · Pause and Stop (App Intents) |
| Lock Screen | The expanded layout, with the footer "Audio stays on this iPhone" |
| Writing notes | "Writing notes…", then "Notes ready", which deep-links to the note |

## Android: ongoing notification

| Part | Content |
| --- | --- |
| Small icon | The Cilica mark, accent colour `rec` |
| Title / text | The note title / "Recording · " + a chronometer |
| Actions | Pause, Stop. Tapping the body opens the note |
| Android 16+ | Promoted Live Update: a status-bar chip with the dot and timer, and a lock-screen card |
| After | "Writing notes…" with an indeterminate bar, then a separate "Notes ready" notification |

Both pause automatically when a phone call starts, and show "Paused by a call". Neither resumes on its own.
