# RecorderCapsule

**Where recording starts.** A note's title band holds the record control (`RecordButton`), beside the Notes / Transcript switch: a microphone and "Record meeting" at rest, a breathing red stop square with the timer while live (a click stops; no "Stop" label, the square says it), "Writing notes…" while finishing. When the band is narrower than 640px it shrinks to its icon; below 520px the switch shrinks to icons too. After a meeting, the recording's player sits in the same band (play / pause with the time, and an info button whose popover has the scrubber, skip, speed and the recording's details); below 760px it shrinks to its icon. On a note the capsule below only appears for the auto-stop countdown; on other pages it shows while recording or finishing. It never shows at rest.

The recorder capsule is the in-app recording control, docked bottom-centre of the note, 24px above the window edge.

**Idle**: a glass pill, "Start recording" with a still `rec` dot and ⌘⇧R.
**Live**: breathing `rec` dot, Geist Mono timer, a 12-bar level meter (3px bars, heights follow the mixed input level at about 10fps, 90ms linear), and the Stop button (`rec` fill, white square).
**Hover or focus**: the capsule widens with `--spring-bouncy` to reveal the input sources (mic, system app) and Pause.

**Start morph**: the idle pill's dot travels to the left, the label fades out and the timer and meter fade in while the width springs (`--spring-bouncy`). The start sound plays at the same moment.

**Stop**: the capsule shrinks to a check, shows "Writing notes…" with the shimmer, then collapses back to idle when the summary is ready (notes-ready sound).

Glass only: it floats over the note. It never covers text: notes keep `space-16` of bottom runway.
