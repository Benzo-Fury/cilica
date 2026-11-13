/**
 * Recording, transcripts, speakers, Saved people, permissions, input devices and speech models.
 *
 * A fake recording plays a canned conversation: segments arrive as growing partials then finals,
 * audio levels tick at 20 Hz, and Stop streams a summary into the note word by word. As the
 * backend, speakers come from Cilica by default: a line has no speaker ("Pending") until a moment
 * after it is final, then the call alternates between Person 1 and Person 2.
 *
 * A recording started for a detected meeting plays the script once; then the meeting app
 * "releases the mic" and `auto_stop_at` counts down 10 s before it stops on its own (Keep recording
 * cancels it). `simulate` drives detection by hand from the console (`__cilicaMock.simulate`).
 * Permissions: the microphone and system audio are granted on request; Accessibility needs a trip
 * to System Settings, which "grants" it a moment after Open Settings.
 */
import type {
  AiAction,
  AudioChannel,
  AudioDevice,
  HardwareProfile,
  Note,
  PermissionKind,
  PermissionStatus,
  Person,
  RecordingState,
  Segment,
  Speaker,
  SpeakerModel,
  SpeakerSource,
  SpeakerSuggestion,
  StartRecordingOptions,
  SttEngine,
  SttModel,
  SttProviderStatus,
  Transcript,
  TranscriptionSettings,
  TranscriptionTest,
} from "@/ipc/types";
import { requireLiveNote, requireNote, writeBody } from "./notes";
import type { MockContext } from "./state";
import { idleRecording } from "./state";
import { Timers } from "./timers";
import { appError, clock, iso, newId, notFound, wordChunks } from "./util";

/** How long after a final line Cilica's speaker detection names who said it. */
const PENDING_MS = 1_800;

/** Who actually labels speakers live (crates/domain: `speaker_labels`). */
export function speakerLabels(t: TranscriptionSettings): SpeakerSource {
  const serviceCan = ["soniox", "deepgram", "assembly_ai"].includes(t.engine);
  return t.speaker_source === "service" && !serviceCan ? "local" : t.speaker_source;
}

/** The canned conversation a fake recording plays, in order. */
const SCRIPT: { channel: AudioChannel; text: string }[] = [
  {
    channel: "mic",
    text: "Okay, let's get going. The goal today is to agree on the beta launch date.",
  },
  {
    channel: "system",
    text: "I think the twenty-fourth still works if we freeze features this Friday.",
  },
  { channel: "mic", text: "What's left on the list before we can freeze?" },
  { channel: "system", text: "Honestly? The empty states are still a bloody mess." },
  { channel: "mic", text: "Oh, for fuck's sake. Again?" },
  {
    channel: "system",
    text: "Two things: the export to Markdown and the empty states in the sidebar.",
  },
  { channel: "system", text: "Export is nearly done. I can have it in review by Thursday." },
  {
    channel: "mic",
    text: "Great. I'll take the empty states then, and I'll write the launch email.",
  },
  { channel: "system", text: "Should we invite the waitlist all at once or in batches?" },
  { channel: "mic", text: "Batches of two hundred a day. That keeps support manageable." },
  {
    channel: "system",
    text: "Agreed. I'll set up the invite script and a dashboard for sign-ups.",
  },
  {
    channel: "mic",
    text: "Last question: who's on support the first week? Let's decide that tomorrow.",
  },
];

const STEP_MS = 350;
const PAUSE_MS = 1200;
const PHASE_MS = 600;
const STREAM_MS = 40;

/** What the transcription test's sample clip says (crates/stt/src/sample.rs). */
const SAMPLE_TEXT =
  "And so, my fellow Americans, ask not what your country can do for you, ask what you can do for your country.";
const TEST_FIRST_WORDS_MS = 600;

export function recordingCommands({ state, emit }: MockContext) {
  const timers = new Timers();
  let levelTimer: ReturnType<typeof setInterval> | null = null;
  let scriptIndex = 0;
  let speaking: AudioChannel | null = null;
  let startedMs = 0;
  let levels = { mic: 0, system: 0 };
  const downloads = new Map<string, ReturnType<typeof setInterval>>();
  /** Called when a download finishes (download-and-use). */
  const onDownloaded = new Map<string, () => void>();
  /** Cloud speech services with a key in the "keychain". */
  const sttKeys = new Set<SttEngine>();
  /** Call lines heard so far in this recording, to alternate the two people on the call. */
  let callLines = 0;
  const switchToSpeakerModel = (modelId: string) => {
    state.settings = {
      ...state.settings,
      transcription: {
        ...state.settings.transcription,
        speaker_source: "local",
        speaker_model: modelId,
      },
    };
    emit({ type: "settings_changed", settings: JSON.parse(JSON.stringify(state.settings)) });
  };
  const switchToSttModel = (modelId: string) => {
    state.settings = {
      ...state.settings,
      transcription: {
        ...state.settings.transcription,
        engine: "whisper_local",
        model_id: modelId,
      },
    };
    emit({ type: "settings_changed", settings: JSON.parse(JSON.stringify(state.settings)) });
  };
  let detected = false;
  let autoStopTimer: ReturnType<typeof setTimeout> | null = null;

  const scheduleAutoStop = () => {
    if (state.recording.phase !== "recording") return;
    const at = Date.now() + 10_000;
    setRecording({ auto_stop_at: iso(at) });
    const app = state.recording.meeting_app ?? "The meeting app";
    emit({ type: "meeting_ended", app_name: app });
    emit({
      type: "toast",
      toast: {
        kind: "action",
        title: `${app} meeting ended`,
        description: "Recording stops in 10 seconds.",
        action_label: "Keep recording",
        action: { type: "keep_recording" },
      },
    });
    autoStopTimer = timers.later(10_000, () => {
      autoStopTimer = null;
      if (state.recording.phase === "recording" && state.recording.auto_stop_at)
        commands.stopRecording();
    });
  };

  const setRecording = (patch: Partial<RecordingState>) => {
    state.recording = { ...state.recording, ...patch };
    emit({ type: "recording_state_changed", state: { ...state.recording } });
  };

  // ── Live transcript ──

  const liveTranscript = (noteId: string): Transcript => {
    let transcript = state.transcripts.get(noteId);
    if (!transcript) {
      transcript = {
        note_id: noteId,
        speakers: [],
        segments: [],
        duration_ms: null,
        has_audio: true,
        timed: true,
        quotes: [],
      };
      state.transcripts.set(noteId, transcript);
    }
    for (const [label, isSelf, color] of [
      ["You", true, 1],
      ["Others", false, 2],
    ] as const) {
      if (!transcript.speakers.some((s) => s.label === label)) {
        transcript.speakers.push({
          id: newId("spk"),
          note_id: noteId,
          label,
          person_id: null,
          color_index: color,
          is_self: isSelf,
        });
      }
    }
    return transcript;
  };

  const upsertSegment = (transcript: Transcript, segment: Segment) => {
    const index = transcript.segments.findIndex((s) => s.id === segment.id);
    if (index >= 0) transcript.segments[index] = segment;
    else transcript.segments.push(segment);
    emit({ type: "transcript_segment", segment: { ...segment } });
  };

  const playUtterance = (noteId: string) => {
    if (detected && scriptIndex >= SCRIPT.length) {
      scheduleAutoStop();
      return;
    }
    const line = SCRIPT[scriptIndex % SCRIPT.length];
    if (!line || state.recording.phase !== "recording") return;
    scriptIndex += 1;
    const transcript = liveTranscript(noteId);
    const labels = speakerLabels(state.settings.transcription);
    const fallback = transcript.speakers.find(
      (s) => s.label === (line.channel === "mic" ? "You" : "Others"),
    );
    const person =
      labels === "off" || line.channel === "mic"
        ? fallback
        : personSpeaker(transcript, (callLines++ % 2) + 1);
    // Cilica's own detection decides a moment after the words; a service labels at once.
    const speaker = labels === "local" ? undefined : person;
    const words = line.text.split(" ");
    const start = Date.now() - startedMs;
    const id = newId("seg");
    const make = (count: number, final: boolean): Segment => ({
      id,
      note_id: noteId,
      speaker_id: speaker?.id ?? null,
      channel: line.channel,
      start_ms: start,
      end_ms: start + count * 280,
      text: words.slice(0, count).join(" "),
      is_final: final,
    });
    speaking = line.channel;
    const steps = [0.4, 0.75, 1].map((f) => Math.max(1, Math.round(words.length * f)));
    steps.forEach((count, i) => {
      timers.later(i * STEP_MS, () => {
        if (state.recording.phase === "recording") upsertSegment(transcript, make(count, false));
      });
    });
    timers.later(steps.length * STEP_MS, () => {
      if (state.recording.phase !== "recording") return;
      upsertSegment(transcript, make(words.length, true));
      if (labels === "local")
        timers.later(PENDING_MS, () => {
          upsertSegment(transcript, {
            ...make(words.length, true),
            speaker_id: person?.id ?? null,
          });
        });
      speaking = null;
      timers.later(PAUSE_MS, () => playUtterance(noteId));
    });
  };

  /** "Person n" on this note, added the first time they are heard. */
  const personSpeaker = (transcript: Transcript, n: number): Speaker => {
    const label = `Person ${n}`;
    let speaker = transcript.speakers.find((s) => s.label === label);
    if (!speaker) {
      speaker = {
        id: newId("spk"),
        note_id: transcript.note_id,
        label,
        person_id: null,
        color_index: n + 1,
        is_self: false,
      };
      transcript.speakers.push(speaker);
      emit({ type: "data_changed", scopes: ["transcript"], note_id: transcript.note_id });
    }
    return speaker;
  };

  const tickLevels = () => {
    const target = (channel: AudioChannel) =>
      speaking === channel ? 0.35 + Math.random() * 0.6 : Math.random() * 0.08;
    levels = {
      mic: levels.mic * 0.55 + target("mic") * 0.45,
      system: levels.system * 0.55 + target("system") * 0.45,
    };
    emit({ type: "audio_levels", levels: { ...levels } });
  };

  const stopCapture = () => {
    timers.stop(levelTimer);
    levelTimer = null;
    speaking = null;
    timers.clear();
  };

  // ── Summary ──

  const summaryFor = (note: Note, minutes: number): string => {
    if (note.body_md.startsWith("## Summary")) return note.body_md;
    return `## Summary

The team agreed to launch the beta on the 24th with a feature freeze this Friday. Export to Markdown and the sidebar empty states are the last two items. Invites go out to the waitlist in batches of 200 a day.

## Decisions

- Beta launch on the **24th**; feature freeze on Friday.
- Invite the waitlist in batches of 200 a day.

## Action items

- [ ] Finish export to Markdown and put it in review (Others, Thursday)
- [ ] Design the sidebar empty states (You, Friday)
- [ ] Write the launch email (You, before the 24th)
- [ ] Set up the invite script and a sign-ups dashboard (Others)

## Open questions

- Who is on support during the first week?

_From ${minutes} ${minutes === 1 ? "minute" : "minutes"} of conversation._
`;
  };

  /** Stream a summary into the note, then store it. Calls `onDone` with the action-item count. */
  const streamSummary = (
    note: Note,
    minutes: number,
    jottings: string,
    onDone: (actionItems: number) => void,
  ) => {
    let body = summaryFor(note, minutes);
    if (jottings.trim() && !body.includes(jottings.trim()))
      body += `\n## Your notes\n\n${jottings.trim()}\n`;
    const chunks = wordChunks(body, 3);
    let sent = "";
    let index = 0;
    const tick = timers.every(STREAM_MS, () => {
      const chunk = chunks[index];
      index += 1;
      if (chunk !== undefined) sent += chunk;
      const done = index >= chunks.length;
      emit({ type: "note_body_stream", note_id: note.id, body_md: done ? body : sent, done });
      if (!done) return;
      timers.stop(tick);
      const action: AiAction = {
        id: newId("act"),
        thread_id: null,
        turn_id: newId("turn"),
        tool: "edit_note",
        tier: "edit",
        args_json: JSON.stringify({ note_id: note.id, section: "Summary" }),
        summary: `Wrote the summary for ${note.title || "Untitled"}`,
        status: "applied",
        error: null,
        can_undo: true,
        proposal_id: null,
        created_at: iso(),
      };
      state.aiActions.unshift(action);
      writeBody(state, note, body, action.id);
      note.snippet = "Beta launches on the 24th; invites go out in batches of 200 a day.";
      emit({
        type: "data_changed",
        scopes: ["notes", "transcript", "versions", "ai_actions"],
        note_id: note.id,
      });
      onDone((body.match(/^- \[ \]/gm) ?? []).length);
    });
  };

  // ── Speakers and people helpers ──

  const findSpeaker = (speakerId: string): Speaker => {
    for (const t of state.transcripts.values()) {
      const s = t.speakers.find((x) => x.id === speakerId);
      if (s) return s;
    }
    throw notFound("speaker");
  };
  const requirePerson = (personId: string): Person => {
    const person = state.people.find((p) => p.id === personId);
    if (!person) throw notFound("person");
    return person;
  };
  const allSpeakers = () => [...state.transcripts.values()].flatMap((t) => t.speakers);

  const commands = {
    getRecordingState(): RecordingState {
      return state.recording;
    },

    startRecording(options: StartRecordingOptions): RecordingState {
      if (state.recording.phase !== "idle") {
        throw appError(
          "conflict",
          "Cilica is already recording. Stop the current recording first.",
        );
      }
      const now = Date.now();
      const app = options.meeting_app ?? null;
      let note: Note;
      if (options.note_id) {
        note = requireLiveNote(state, options.note_id);
        if (note.kind !== "meeting") {
          note.kind = "meeting";
          note.meeting_app = app;
        }
        note.started_at = note.started_at ?? iso(now);
      } else {
        const positions = [...state.notes.values()]
          .filter((n) => !n.deleted_at && n.category_id === null)
          .map((n) => n.position ?? 0);
        note = {
          id: newId("note"),
          title: app ? `${app} meeting · ${clock(now)}` : `Meeting · ${clock(now)}`,
          body_md: "",
          snippet: "",
          category_id: null,
          kind: "meeting",
          meeting_app: app,
          started_at: iso(now),
          ended_at: null,
          position: positions.length ? Math.min(...positions) - 1 : 1,
          created_at: iso(now),
          updated_at: iso(now),
          deleted_at: null,
        };
        state.notes.set(note.id, note);
      }
      liveTranscript(note.id);
      startedMs = now;
      scriptIndex = 0;
      callLines = 0;
      detected = options.trigger === "detected";
      emit({ type: "data_changed", scopes: ["notes"], note_id: note.id });
      const t = state.settings.transcription;
      setRecording({
        phase: "starting",
        note_id: note.id,
        meeting_app: app,
        started_at: iso(now),
        auto_stop_at: null,
        live_speakers: speakerLabels(t) !== "off",
      });
      const noteId = note.id;
      const title = note.title;
      timers.later(250, () => {
        if (state.recording.phase !== "starting") return;
        setRecording({ phase: "recording" });
        // The backend raises "Recording" only when the UI did not start it (hotkey, tray, detection).
        if (options.trigger !== "manual")
          emit({
            type: "toast",
            toast: {
              kind: "action",
              title: "Recording",
              description: title,
              action_label: "Stop",
              action: { type: "stop_recording" },
            },
          });
        levelTimer = timers.every(50, tickLevels);
        timers.later(600, () => playUtterance(noteId));
      });
      return state.recording;
    },

    stopRecording(): RecordingState {
      const { phase, note_id: noteId } = state.recording;
      if ((phase !== "recording" && phase !== "starting") || !noteId) {
        throw appError("invalid", "Nothing is recording right now.");
      }
      stopCapture();
      emit({ type: "audio_levels", levels: { mic: 0, system: 0 } });
      const note = requireNote(state, noteId);
      const minutes = Math.max(1, Math.round((Date.now() - startedMs) / 60_000));
      note.ended_at = iso();
      const transcript = state.transcripts.get(noteId);
      if (transcript) {
        transcript.segments = transcript.segments.filter((s) => s.is_final);
        transcript.duration_ms = Date.now() - startedMs;
      }
      setRecording({ phase: "stopping", auto_stop_at: null });
      // Lines still pending go to You or Others; speakers left with nothing said are dropped.
      if (transcript) {
        const fallback = (channel: AudioChannel) =>
          transcript.speakers.find((p) => p.label === (channel === "mic" ? "You" : "Others"))?.id ??
          null;
        for (const s of transcript.segments) s.speaker_id ??= fallback(s.channel);
        transcript.speakers = transcript.speakers.filter((p) =>
          transcript.segments.some((s) => s.speaker_id === p.id),
        );
      }
      timers.later(PHASE_MS, () => {
        setRecording({ phase: "summarizing" });
        streamSummary(note, minutes, note.body_md, (actionItems) => {
          state.recording = idleRecording();
          emit({ type: "recording_state_changed", state: { ...state.recording } });
          emit({
            type: "toast",
            toast: {
              kind: "success",
              title: "Notes ready",
              description: `Summary and ${actionItems} action ${actionItems === 1 ? "item" : "items"} from ${minutes} ${minutes === 1 ? "minute" : "minutes"}.`,
              action_label: "Open",
              action: { type: "open_note", note_id: noteId },
            },
          });
        });
      });
      return state.recording;
    },

    keepRecording(): RecordingState {
      if (autoStopTimer) timers.stop(autoStopTimer);
      autoStopTimer = null;
      detected = false;
      if (state.recording.auto_stop_at) setRecording({ auto_stop_at: null });
      if (state.recording.phase === "recording" && state.recording.note_id) {
        const noteId = state.recording.note_id;
        timers.later(PAUSE_MS, () => playUtterance(noteId));
      }
      return state.recording;
    },

    regenerateSummary(noteId: string): null {
      const note = requireLiveNote(state, noteId);
      if (state.recording.note_id === noteId) {
        throw appError("conflict", "This note is still recording. Stop the recording first.");
      }
      const transcript = state.transcripts.get(noteId);
      if (!transcript || transcript.segments.length === 0) {
        throw appError("unavailable", "This note has no transcript to summarise.");
      }
      const minutes = Math.max(1, Math.round((transcript.duration_ms ?? 60_000) / 60_000));
      streamSummary(note, minutes, "", () => {});
      return null;
    },

    // ── Devices and permissions ──

    listInputDevices(): AudioDevice[] {
      return [
        { id: "builtin", name: "MacBook Pro Microphone", is_default: true },
        { id: "usb-mv7", name: "Shure MV7", is_default: false },
      ];
    },

    getPermissions() {
      return state.permissions;
    },

    requestPermission(kind: PermissionKind): PermissionStatus {
      // Accessibility can only be granted in System Settings.
      const status: PermissionStatus = kind === "accessibility" ? "denied" : "granted";
      state.permissions = { ...state.permissions, [kind]: status };
      return status;
    },

    openPermissionSettings(kind: PermissionKind): null {
      timers.later(1500, () => {
        state.permissions = { ...state.permissions, [kind]: "granted" };
      });
      return null;
    },

    // ── Speech models ──

    listSttModels(): SttModel[] {
      return state.sttModels;
    },

    downloadSttModel(modelId: string): null {
      const model: SttModel | SpeakerModel | undefined =
        state.sttModels.find((m) => m.id === modelId) ??
        state.speakerModels.find((m) => m.id === modelId);
      if (!model) throw notFound("model");
      if (model.downloaded || downloads.has(modelId)) return null;
      const total = model.size_mb * 1024 * 1024;
      let step = 0;
      const steps = 12;
      const handle = setInterval(() => {
        step += 1;
        const done = step >= steps;
        emit({
          type: "model_download",
          progress: {
            model_id: modelId,
            downloaded_bytes: Math.round((total * step) / steps),
            total_bytes: total,
            done,
            error: null,
          },
        });
        if (done) {
          clearInterval(handle);
          downloads.delete(modelId);
          model.downloaded = true;
          onDownloaded.get(modelId)?.();
          onDownloaded.delete(modelId);
        }
      }, 250);
      downloads.set(modelId, handle);
      return null;
    },

    downloadAndUseSttModel(modelId: string): null {
      const model = state.sttModels.find((m) => m.id === modelId);
      if (!model) throw notFound("model");
      if (model.downloaded) {
        switchToSttModel(modelId);
        return null;
      }
      onDownloaded.set(modelId, () => switchToSttModel(modelId));
      return commands.downloadSttModel(modelId);
    },

    getHardwareProfile(): HardwareProfile {
      return {
        cores: 10,
        ram_gb: 24,
        apple_silicon: true,
        suggest_cloud: false,
        local_speakers: true,
      };
    },

    listSpeakerModels(): SpeakerModel[] {
      return state.speakerModels;
    },

    downloadAndUseSpeakerModel(modelId: string): null {
      const model = state.speakerModels.find((m) => m.id === modelId);
      if (!model) throw notFound("model");
      if (model.downloaded) {
        switchToSpeakerModel(modelId);
        return null;
      }
      onDownloaded.set(modelId, () => switchToSpeakerModel(modelId));
      return commands.downloadSttModel(modelId);
    },

    getSttProviderStatus(engine: SttEngine): SttProviderStatus {
      return { engine, has_key: sttKeys.has(engine), problem: null };
    },

    /** Keys containing "bad" are rejected, as a provider would. */
    setSttProviderKey(engine: SttEngine, key: string): SttProviderStatus {
      if (!key.trim()) throw appError("invalid", "Paste a key first.");
      if (/bad/i.test(key)) {
        throw appError(
          "permission_denied",
          "The service did not accept the API key. Paste a valid key in Settings › Transcription.",
        );
      }
      sttKeys.add(engine);
      return { engine, has_key: true, problem: null };
    },

    clearSttProviderKey(engine: SttEngine): SttProviderStatus {
      sttKeys.delete(engine);
      return { engine, has_key: false, problem: null };
    },

    /** Plays the sample's words back over about three seconds, as a service would. */
    testTranscription(): Promise<TranscriptionTest> {
      const engine = state.settings.transcription.engine;
      if (state.recording.phase !== "idle") {
        throw appError("invalid", "Finish the recording first, then run the test.");
      }
      if (engine !== "whisper_local" && !sttKeys.has(engine)) {
        throw appError(
          "unavailable",
          "There is no API key for this service. Add one in Settings › Transcription.",
        );
      }
      const words = SAMPLE_TEXT.split(" ");
      const step = 3_000 / words.length;
      return new Promise((resolve) => {
        words.forEach((_, i) => {
          timers.later(TEST_FIRST_WORDS_MS + i * step, () =>
            emit({ type: "transcription_test_progress", text: words.slice(0, i + 1).join(" ") }),
          );
        });
        timers.later(TEST_FIRST_WORDS_MS + words.length * step, () =>
          resolve({
            engine,
            text: SAMPLE_TEXT,
            expected: SAMPLE_TEXT,
            first_words_ms: TEST_FIRST_WORDS_MS,
            took_ms: Math.round(TEST_FIRST_WORDS_MS + words.length * step),
          }),
        );
      });
    },

    cancelSttModelDownload(modelId: string): null {
      onDownloaded.delete(modelId);
      const handle = downloads.get(modelId);
      if (!handle) return null;
      clearInterval(handle);
      downloads.delete(modelId);
      emit({
        type: "model_download",
        progress: {
          model_id: modelId,
          downloaded_bytes: null,
          total_bytes: null,
          done: false,
          error: "Download cancelled.",
        },
      });
      return null;
    },

    deleteSttModel(modelId: string): null {
      const voice = state.speakerModels.find((m) => m.id === modelId);
      if (voice) {
        if (!voice.removable) throw appError("invalid", "Cilica needs this voice model.");
        if (state.settings.transcription.speaker_model === modelId) {
          throw appError(
            "invalid",
            "That model is in use. Pick another model first, then delete this one.",
          );
        }
        voice.downloaded = false;
        return null;
      }
      const model = state.sttModels.find((m) => m.id === modelId);
      if (!model) throw notFound("model");
      if (state.settings.transcription.model_id === modelId) {
        throw appError(
          "invalid",
          "That model is in use. Pick another model first, then delete this one.",
        );
      }
      model.downloaded = false;
      return null;
    },

    // ── Transcripts and speakers ──

    getTranscript(noteId: string): Transcript {
      requireNote(state, noteId);
      return (
        state.transcripts.get(noteId) ?? {
          note_id: noteId,
          speakers: [],
          segments: [],
          duration_ms: null,
          has_audio: false,
          timed: true,
          quotes: [],
        }
      );
    },

    /** The preview has no audio files: a `mock:` source plays as a silent clock. */
    getAudioPath(noteId: string): string | null {
      requireNote(state, noteId);
      return state.transcripts.get(noteId)?.has_audio ? `mock:audio/${noteId}` : null;
    },

    renameSpeaker(speakerId: string, label: string): Speaker {
      const speaker = findSpeaker(speakerId);
      const trimmed = label.trim();
      if (!trimmed) throw appError("invalid", "Give the speaker a name.");
      speaker.label = trimmed;
      return speaker;
    },

    linkSpeaker(speakerId: string, personId: string | null): Speaker {
      const speaker = findSpeaker(speakerId);
      if (personId === null) {
        speaker.person_id = null;
        return speaker;
      }
      const person = requirePerson(personId);
      speaker.person_id = person.id;
      speaker.label = person.name;
      speaker.color_index = person.color_index;
      person.sample_count += 1;
      return speaker;
    },

    saveSpeakerAsPerson(speakerId: string, name: string): Person {
      const speaker = findSpeaker(speakerId);
      const trimmed = name.trim();
      if (!trimmed) throw appError("invalid", "Give the person a name.");
      const person: Person = {
        id: newId("person"),
        name: trimmed,
        color_index: speaker.color_index,
        sample_count: 1,
        meeting_count: 1,
        created_at: iso(),
        is_self: false,
      };
      state.people.push(person);
      speaker.person_id = person.id;
      speaker.label = person.name;
      return person;
    },

    listSpeakerSuggestions(noteId: string): SpeakerSuggestion[] {
      requireNote(state, noteId);
      const transcript = state.transcripts.get(noteId);
      if (!transcript) return [];
      if (state.recording.note_id === noteId && state.recording.phase !== "idle") return [];
      const linked = new Set(transcript.speakers.map((s) => s.person_id).filter(Boolean));
      const speaker = transcript.speakers.find(
        (s) => !s.is_self && !s.person_id && /^(speaker|person) \d+$/i.test(s.label),
      );
      // You are the mic speaker (linked to You), so never suggested for anyone else.
      const person = state.people.find((p) => !linked.has(p.id) && !p.is_self);
      if (!speaker || !person) return [];
      return [
        {
          speaker_id: speaker.id,
          person_id: person.id,
          person_name: person.name,
          similarity: 0.82,
        },
      ];
    },

    // ── Saved people ──

    listPeople(): Person[] {
      return [...state.people].sort(
        (a, b) => Number(b.is_self) - Number(a.is_self) || a.name.localeCompare(b.name),
      );
    },

    renamePerson(personId: string, name: string): Person {
      const person = requirePerson(personId);
      const trimmed = name.trim();
      if (!trimmed) throw appError("invalid", "Give the person a name.");
      person.name = trimmed;
      for (const s of allSpeakers()) if (s.person_id === personId) s.label = trimmed;
      return person;
    },

    setPersonColor(personId: string, colorIndex: number): Person {
      const person = requirePerson(personId);
      if (!Number.isInteger(colorIndex) || colorIndex < 1 || colorIndex > 8) {
        throw appError("invalid", "Pick one of the eight speaker colours.");
      }
      person.color_index = colorIndex;
      for (const s of allSpeakers()) if (s.person_id === personId) s.color_index = colorIndex;
      return person;
    },

    mergePeople(sourceId: string, targetId: string): Person {
      if (sourceId === targetId) throw appError("invalid", "Pick two different people to merge.");
      const source = requirePerson(sourceId);
      if (source.is_self) {
        throw appError("invalid", "You are always kept. Merge the other person into You instead.");
      }
      const target = requirePerson(targetId);
      target.sample_count += source.sample_count;
      target.meeting_count += source.meeting_count;
      for (const s of allSpeakers()) {
        if (s.person_id === sourceId) {
          s.person_id = targetId;
          s.label = target.name;
          s.color_index = target.color_index;
        }
      }
      state.people = state.people.filter((p) => p.id !== sourceId);
      return target;
    },

    deletePerson(personId: string): null {
      if (requirePerson(personId).is_self) {
        throw appError(
          "invalid",
          "You are always kept. To start your voice afresh, choose Forget voice.",
        );
      }
      for (const s of allSpeakers()) if (s.person_id === personId) s.person_id = null;
      state.people = state.people.filter((p) => p.id !== personId);
      return null;
    },

    forgetPersonVoice(personId: string): Person {
      const person = requirePerson(personId);
      person.sample_count = 0;
      return person;
    },

    /** Test helper: stop every timer. */
    dispose(): void {
      timers.clear();
      for (const handle of downloads.values()) clearInterval(handle);
      downloads.clear();
    },
  };

  /** Drive meeting detection by hand (dev console, tests). */
  const simulate = {
    meetingDetected(appName = "Zoom") {
      const meeting = { app_name: appName, app_id: null };
      emit({ type: "meeting_detected", meeting });
      if (
        state.settings.recording.detect_mode === "auto_start" &&
        state.recording.phase === "idle"
      ) {
        commands.startRecording({ note_id: null, meeting_app: appName, trigger: "detected" });
        return;
      }
      emit({
        type: "toast",
        toast: {
          kind: "action",
          title: `${appName} meeting detected`,
          description: "Record it to get notes when it ends.",
          action_label: "Record",
          action: { type: "start_recording", meeting_app: appName },
        },
      });
    },
    meetingEnded() {
      scheduleAutoStop();
    },
  };

  return { ...commands, simulate };
}
