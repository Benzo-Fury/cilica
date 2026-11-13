# cilica-scheduler

Scheduled tasks: CRUD, the default Notes review (every 3 days at 09:00, seeded on first launch), and the runner.

- `schedule::next_run`: local wall-clock, DST-safe (gaps move forward, repeats use the first).
- `Scheduler::init` fails runs interrupted by a quit, seeds the default, schedules tasks without a next run. `start` ticks every 30 s; missed runs run once at launch.
- Never runs while recording (`is_recording` closure from the shell); a run in progress is cancelled when a recording starts. One run at a time; `run_now` runs in the background; `cancel_current`.
- Runs use the agent in propose-only mode; finishing emits `TaskRunFinished`, `DataChanged` and a toast with `OpenTaskRun`.

Must not depend on tauri. Test with `cargo test -p cilica-scheduler` (chrono-tz zones for DST cases, MockProvider for runs).
