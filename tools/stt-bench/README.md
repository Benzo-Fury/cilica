# Speech engine benchmark

Compares local speech-to-text engines on real meeting audio, the way Margin uses them: live
(text on screen while people talk) and after the meeting (one careful pass over the
recording). Nothing here ships in the app.

## What it measures

- **WER**: word error rate of the final transcript after Whisper's English text normaliser
  (the Open ASR Leaderboard convention), split into substitutions, deletions and insertions.
- **Shown lag**: how long after a word is spoken it first appears on screen (live text
  included). Median and 90th percentile, measured on correctly recognised words.
- **Final lag**: the same for text that will not change any more.
- **RTF**: compute time / audio time. Above 1 an engine cannot keep up live; for a live
  engine, keep well under 0.5 so the rest of the app (and a second channel) has room.
- **Backlog at end**: how long after the meeting ended the last live text arrived.
- **Peak RAM**: maximum resident memory of the run.

Live runs use a simulated clock: a 100 ms chunk can be processed once it has been spoken
and the previous chunk is finished, so a slow engine falls behind exactly as it would live
without making the benchmark wait in real time. Apple SpeechAnalyzer is asynchronous, so its
live run is fed in real time instead.

## Test audio

Four 10-minute clips from the AMI Meeting Corpus test set (CC BY 4.0,
`edinburghcstr/ami` on Hugging Face), meetings EN2002a and EN2002b, each in two forms:

- `ihm`: the participants' headset microphones mixed (close to call audio)
- `sdm`: one far-field microphone on the table (close to a laptop mic in a room)

Utterances are put back on the meeting timeline so engines hear continuous audio. EN2002
meetings are spontaneous, with a lot of overlapping speech; no single-stream engine can
transcribe two people at once, so absolute WERs are high for every engine. Compare engines
with each other, not with leaderboard numbers.

## Engines

`python -I run.py list` prints them all. Runners:

| Runner | Engines |
| --- | --- |
| `margin-runner/` (Rust) | Margin's own `margin_stt::WhisperLocal` stream, unchanged; whisper.cpp whole-file pass |
| `runner_sherpa.py` | sherpa-onnx on the CPU: Nemotron streaming, Parakeet TDT v3, Qwen3-ASR 0.6B |
| `runner_mlx.py` | parakeet-mlx on Apple's GPU |
| `runner_mlxaudio.py` | mlx-audio on Apple's GPU: Voxtral Mini 4B Realtime, Qwen3-ASR 1.7B |
| `runner_moonshine.py` | moonshine-voice: Moonshine v2 streaming |
| `apple-runner/` (Swift) | Apple SpeechAnalyzer (macOS 26+) |

Strategies: `native`/`continuous` use an engine's own streaming; `chunked` wraps a
clip-at-a-time model in Margin's live strategy (`common.py` ports `crates/stt/src/vad.rs`
exactly); `offline` is the after-the-meeting pass.

## Hardware profiles

| Profile | Meaning | Stands in for |
| --- | --- | --- |
| `accel` | GPU / Neural Engine where the engine has it | Apple silicon Mac |
| `cpu4` | CPU only, 4 threads | typical laptop or desktop PC |
| `old2` | CPU only, 2 threads, efficiency cores only (`taskpolicy -b`) | old PC |

These are stand-ins measured on one Mac. To get real numbers, run the same commands on the
real machine (the Python runners and sherpa-onnx work on Windows and Linux; drop the
`taskpolicy` prefix in `run.py` there).

## Running it

```bash
python3.14 -m venv .venv
.venv/bin/pip install pyarrow numpy soundfile jiwer whisper-normalizer sherpa-onnx onnxruntime parakeet-mlx mlx-audio moonshine-voice
```

Then fetch the AMI shards and models into `.cache/` (see `prepare.py` and `engines.py` for
paths), build the native runners and run:

```bash
(cd margin-runner && cargo build --release)
(cd apple-runner && swiftc -parse-as-library -O main.swift -o apple-runner)
.venv/bin/python -I prepare.py .cache
./bench_all.sh
.venv/bin/python -I run.py summary
```

`python -I smoke.py <engine>` checks one engine on 20 seconds of audio.

## Licences of the models tested

| Model | Licence | Shipping it in Margin means |
| --- | --- | --- |
| Whisper (whisper.cpp GGML) | MIT | nothing extra |
| Parakeet TDT 0.6B v3 | CC BY 4.0 | credit NVIDIA, link the licence, say it was converted (int8 / MLX) |
| Nemotron Speech Streaming EN 0.6B, parakeet realtime EOU | NVIDIA Open Model License | include the agreement and a notice "Licensed by NVIDIA Corporation under the NVIDIA Open Model License"; rights end if its guardrails are bypassed |
| Nemotron 3.5 ASR Streaming 0.6B | OpenMDW-1.1 | keep the licence and origin notices |
| Moonshine v2 streaming (all languages) | MIT | nothing extra (legacy non-streaming non-English Moonshine models are under a community licence: avoid) |
| Qwen3-ASR | Apache-2.0 | keep the licence and notices |
| Voxtral Mini 4B Realtime | Apache-2.0 | keep the licence and notices |
| Kyutai STT | CC BY 4.0 (weights) | credit Kyutai |
| Apple SpeechAnalyzer | macOS SDK | system feature, macOS 26+ only |

None of these needs an account or a click-through to download. All allow commercial use.
