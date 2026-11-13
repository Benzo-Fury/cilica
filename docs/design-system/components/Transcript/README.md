# Transcript

Transcript lines show who said what, when, with the speaker's colour only on their avatar ring.

**Line**: 24px avatar, name in 13px semibold `ink`, timecode in Geist Mono `ink-3`, text in `transcript` 15/23. Consecutive lines by one speaker merge under one header.

**Live**: new words fade in with a 2px blur clearing (`--dur-slow`) in `ink-3`, then settle to `ink` once the segment is final. Auto-scroll follows the newest line until the person scrolls up; then a "Jump to live" glass chip appears at the bottom.

**Playback**: the current word gets an `accent-soft` wash; clicking a timecode seeks.

**Pending lines**: while a meeting records with speakers told apart on this computer, a line Cilica has not placed yet shows "Pending" in `ink-3` with a dashed avatar ring, and moves under its speaker a few seconds later. Pending lines are not counted in the speakers chip.

**Unknown speakers** show "Speaker 3" and a "Name this speaker" chip that assigns or creates a Saved Person (and stores the voiceprint).

## Lines worth keeping

Up to three quotes Cilica picked while writing the meeting notes sit at the bottom of the transcript, each with its speaker and time. Clicking one scrolls to the turn and, when the recording is still on disk, plays from there.
