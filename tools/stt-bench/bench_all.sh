#!/bin/sh
# The full benchmark, one run at a time. Takes a few hours on an M4.
# Clips: all four for the main comparison; two or one where a run is slow or only speed matters.
set -u
cd "$(dirname "$0")"
PY=".venv/bin/python -I"
ALL=en2002a-ihm,en2002a-sdm,en2002b-ihm,en2002b-sdm
TWO=en2002a-ihm,en2002b-sdm
ONE=en2002a-ihm

# Margin today and Whisper variants (GPU) — margin-whisper-turbo-live-en and
# whisper-turbo-offline-en on accel were run first.
$PY run.py run margin-whisper-small-live-en,margin-whisper-base-live-en accel $TWO

# Apple silicon GPU / Neural Engine
$PY run.py run apple-speech-offline,parakeet-v3-mlx-offline,parakeet-v3-mlx-chunked,parakeet-v3-mlx-stream,qwen3-asr-1.7b-mlx-offline accel $ALL
$PY run.py run voxtral-rt-4b-mlx accel $TWO

# CPU, 4 threads (typical laptop / PC)
$PY run.py run nemotron-en-560-continuous,nemotron-en-160-continuous,nemotron-3.5-560-continuous,parakeet-v3-chunked,parakeet-v3-chunked-700,parakeet-v3-offline,moonshine-medium-stream,moonshine-small-stream cpu4 $ALL
$PY run.py run nemotron-en-560-stream,qwen3-asr-0.6b-offline,margin-whisper-small-live-en,margin-whisper-base-live-en cpu4 $TWO
$PY run.py run margin-whisper-turbo-live-en,whisper-turbo-offline-en cpu4 $ONE

# Old PC: 2 threads on efficiency cores (speed and whether it keeps up)
$PY run.py run nemotron-en-560-continuous,nemotron-en-160-continuous,parakeet-v3-chunked-700,parakeet-v3-offline,moonshine-medium-stream,moonshine-small-stream,moonshine-tiny-stream,margin-whisper-base-live-en,margin-whisper-small-live-en old2 $ONE

# Apple live, in real time (10 minutes per clip)
$PY run.py run apple-speech-live accel $TWO
