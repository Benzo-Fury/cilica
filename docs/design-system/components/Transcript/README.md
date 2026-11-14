# Transcript

Transcript lines show who said what, when, with the speaker's colour only on their avatar ring.

**Line**: 24px avatar, name in 13px semibold `ink`, timecode in Geist Mono `ink-3`, text in `transcript` 15/23. Consecutive lines by one speaker merge under one header.

**Live**: new words rise 8px into place on `smooth` while a 2px blur clears (`--dur-slow`), in `ink-3`, then settle to `ink` once the segment is final. Auto-scroll follows the newest line until the person scrolls up; then a "Jump to live" glass chip appears at the bottom.

**Playback**: driven from the playback bar only. Timecodes are plain text and lines are not highlighted as they play; the transcript keeps the playing turn on screen.

**Pending lines**: while a meeting records with speakers told apart on this computer, a line Cilica has not placed yet shows "Pending" in `ink-3` with a dashed avatar ring, and moves under its speaker a few seconds later. Each pending line has a row of its own, so placing one never moves the lines after it. A placed line either keeps its row (the name crossfades from Pending to the speaker, the text stays still) or glides up word by word into the speaker's turn above, on `smooth`, with no copy left behind; Pending's avatar and name fade out on top (`--dur-fast`, ease-in) without taking space. Rows that shift glide instead of jumping, and while live the transcript never gets shorter, so the view never bounces back down. A new row's avatar and name rise with its first words. Nothing animates when a transcript first loads, and Reduce motion makes all of it a plain fade. Pending lines are not counted in the speakers chip.

**Unknown speakers** show "Speaker 3" and a "Name this speaker" chip that assigns or creates a Saved Person (and stores the voiceprint).

## Lines worth keeping

Up to three quotes Cilica picked while writing the meeting notes sit at the bottom of the transcript, each with its speaker and time. Clicking one scrolls to the turn and, when the recording is still on disk, plays from there.
