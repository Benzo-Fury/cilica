#!/bin/sh
# Rebuild the meeting fixtures with macOS tools only (say, afconvert, python3 stdlib).
#   meeting_mic.wav     16 kHz mono: "You" (Samantha) plus a faint, delayed echo of the system
#                       channel, as a laptop mic hears the speakers.
#   meeting_system.wav  16 kHz mono: two remote people (Daniel, Karen).
#   meeting.json        ground truth turns.
set -eu
cd "$(dirname "$0")"
tmp=$(mktemp -d)
i=0
turn() { # channel voice text
  i=$((i + 1))
  say -v "$2" -o "$tmp/$i.aiff" "$3"
  afconvert -f WAVE -d LEI16@16000 -c 1 "$tmp/$i.aiff" "$tmp/$i.wav"
  printf '%s\t%s\t%s\t%s\n' "$i" "$1" "$2" "$3" >> "$tmp/turns.tsv"
}
turn mic Samantha "Hi both, thanks for joining. Shall we start with the launch plan?"
turn system Daniel "Sure. The launch is set for the third of March, and the website goes live the same morning."
turn system Karen "Marketing needs the final copy by the end of next week, otherwise the printed brochures will be late."
turn mic Samantha "Great, I will send the copy on Friday."
turn system Daniel "Perfect. Let's also book a review meeting for Monday morning with the whole team."
turn system Karen "I will send everyone an invitation today, and I will add the agenda to it."
python3 -I - "$tmp" <<'PY'
import json, struct, sys, wave
tmp = sys.argv[1]
rate = 16000
def read(p):
    with wave.open(p) as w:
        b = w.readframes(w.getnframes())
    return [s / 32768.0 for s in struct.unpack("<%dh" % (len(b) // 2), b)]
mic, sysch, truth = [], [], []
def pad(n):
    mic.extend([0.0] * n); sysch.extend([0.0] * n)
pad(int(1.2 * rate))
for line in open(f"{tmp}/turns.tsv"):
    i, ch, voice, text = line.rstrip("\n").split("\t")
    clip = read(f"{tmp}/{i}.wav")
    start = len(mic)
    (mic if ch == "mic" else sysch).extend(clip)
    (sysch if ch == "mic" else mic).extend([0.0] * len(clip))
    truth.append({"channel": ch, "speaker": voice, "start_ms": start * 1000 // rate, "end_ms": len(mic) * 1000 // rate, "text": text})
    pad(int(0.7 * rate))
pad(int(0.8 * rate))
# Speaker leak into the mic: 30 ms later, about -16 dB, slightly smeared.
d = int(0.030 * rate)
for k in range(d + 2, len(mic)):
    mic[k] += 0.12 * sysch[k - d] + 0.04 * sysch[k - d - 2]
def write(p, x):
    with wave.open(p, "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(rate)
        w.writeframes(struct.pack("<%dh" % len(x), *[max(-32768, min(32767, int(round(v * 32767)))) for v in x]))
write("meeting_mic.wav", mic)
write("meeting_system.wav", sysch)
json.dump(truth, open("meeting.json", "w"), indent=1)
print(len(mic) / rate, "s")
PY
rm -rf "$tmp"
