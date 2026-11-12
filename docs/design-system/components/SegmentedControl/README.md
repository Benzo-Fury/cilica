# SegmentedControl

The segmented control switches between two to four views of the same thing, most importantly Notes and Transcript (⌘E).

**Consumer provides**: 2 to 4 short labels (an optional 14px icon each), the selected index, an `onChange`.

**Motion**: the thumb is one element that moves (Motion `layoutId`, or the CSS transform shown) with `--spring-smooth`; labels crossfade colour from `ink-2` to `ink`. The view below changes with `--spring-gentle` (crossfade plus 8px slide toward the selected side).

Also used for Settings › Appearance (Light / Dark / System) and the note font (Sans / Serif / Mono).

**Shape**: the track and the sliding thumb are full pills (`radius-pill`), not squircles.
