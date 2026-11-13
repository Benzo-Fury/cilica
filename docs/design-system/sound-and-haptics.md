# Sound and haptics

Cilica follows what people already expect from meeting and recording tools: an audible cue when recording starts and stops (Zoom, Teams and Meet announce recording; Loom counts in), and a soft chime when notes are ready (Anarlog plays a choosable completion sound). Everything else is silent.

## Sounds

| Moment | Default | Character |
| --- | --- | --- |
| Recording started | On | Two rising soft tones, 250ms. Confirms capture actually began, even if Cilica is behind Zoom. |
| Recording stopped | On | The same two tones, falling. |
| Notes ready | On | One warm chime, 400ms. Choosable in Settings › Sounds (Chime, Bloom, Ready, Glass), or off. |
| Undo, task done, copy | Off | Optional soft ticks under "Interface sounds". |
| Setup background | Off | A quiet ambient pad in the setup window, made with Web Audio. Starts muted; the speaker button in the title band plays it, and it fades out when setup closes. |

- Volume 60%, mixed so it is quieter than a typical notification.
- No sound plays while the person is speaking in a recorded meeting except start and stop (and those are also kept out of the recording: they play after capture starts and before it stops).
- macOS: respect the Focus / Do Not Disturb state for "Notes ready". Start and stop always play, because they are about consent and capture.
- Settings › Sounds has a master switch and a toggle for each row above.

## Haptics (macOS Force Touch trackpads)

Haptics only exist where the OS supports them (`NSHapticFeedbackManager`); Windows and Linux get none.

| Moment | Pattern |
| --- | --- |
| A dragged note or category snaps into its drop position | alignment |
| Sidebar or assistant resize snaps to its default width | alignment |
| A permission is granted during setup | generic |
| Slider detents (playback speed) | levelChange |

Haptics never fire on hover, scroll or typing.
