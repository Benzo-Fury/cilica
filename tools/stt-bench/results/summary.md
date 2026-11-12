| Engine | Profile | Clips | WER | WER close-talk | WER far-field | Del | Ins | Shown lag (med / p90) | Final lag | Backlog at end | RTF | Peak RAM MB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Whisper turbo q5, whole recording, beam 5 (after-meeting pass) | accel | 4 | 34.4% | 33.2% | 36.5% | 22.8% | 2.9% | – / – | – | – | 0.133 | 973 |
| Apple SpeechAnalyzer, whole recording | accel | 4 | 36.2% | 33.5% | 39.7% | 26.9% | 1.0% | – / – | – | – | 0.010 | 87 |
| Parakeet TDT v3 (MLX, GPU), whole recording | accel | 4 | 36.8% | 35.2% | 39.5% | 28.9% | 0.8% | – / – | – | – | 0.028 | 1,579 |
| Parakeet TDT v3 (MLX, GPU), Margin's chunking | accel | 4 | 37.6% | 35.3% | 40.6% | 29.4% | 1.2% | 0.52 / 1.72 | 3.98 | 0.00 | 0.169 | 1,690 |
| Margin pipeline, Whisper small, English | accel | 2 | 43.9% | 35.1% | 54.3% | 31.3% | 1.1% | 0.34 / 1.47 | 4.26 | 0.00 | 0.154 | 810 |
| Margin today, language set to English | accel | 4 | 46.1% | 41.5% | 51.3% | 36.1% | 1.1% | 0.79 / 3.44 | 5.58 | 0.00 | 0.394 | 856 |
| Margin pipeline, Whisper base, English | accel | 2 | 46.3% | 37.0% | 57.1% | 29.8% | 1.6% | 0.35 / 1.62 | 4.14 | 0.00 | 0.064 | 346 |
| Margin today: Whisper turbo q5, live, language auto | accel | 2 | 46.6% | 45.7% | 47.3% | 36.5% | 1.1% | 0.65 / 3.01 | 5.02 | 0.00 | 0.316 | 853 |
| Parakeet TDT v3 (MLX, GPU), native streaming | accel | 1 | 53.1% | 53.1% | – | 39.9% | 1.4% | 131.95 / 217.85 | 496.46 | 218.51 | 1.357 | 973 |
| Voxtral Mini 4B Realtime (MLX 4-bit, GPU), native streaming, 480 ms delay | accel | 2 | 61.3% | 28.4% | 100.0% | 61.8% | 0.2% | 141.11 / 261.57 | 585.67 | 464.92 | 1.775 | 5,780 |
