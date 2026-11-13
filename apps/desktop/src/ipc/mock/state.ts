/** The mock's in-memory state and the event fan-out every module shares. */
import type {
  AiAction,
  BackendEvent,
  Category,
  Message,
  Note,
  NoteVersion,
  Permissions,
  Person,
  Proposal,
  ProviderKind,
  RecordingState,
  Settings,
  SttModel,
  Task,
  TaskRun,
  Thread,
  Transcript,
} from "@/ipc/types";

export interface MockState {
  /** Every note, including the ones in Trash (`deleted_at` set). */
  notes: Map<string, Note>;
  categories: Map<string, Category>;
  versions: NoteVersion[];
  /** By note id. Notes without an entry have no transcript. */
  transcripts: Map<string, Transcript>;
  people: Person[];
  settings: Settings;
  threads: Thread[];
  messages: Message[];
  proposals: Proposal[];
  aiActions: AiAction[];
  /** For undone note edits: the body before and after Cilica's change (the real store keeps
   *  this in `ai_actions.undo_json`, which the UI type does not carry). */
  undoBodies: Map<string, { note_id: string; before_md: string; after_md: string }>;
  /** Recordings older than the seed's notes, so Home's overtime has a usual length to beat. */
  pastMeetings: { id: string; title: string; held_at: string; duration_ms: number }[];
  /** Swears typed into notes (the real store's `swear_coins`), word kept as its root. */
  swearCoins: { id: string; note_id: string | null; word: string; at: string }[];
  /** When the swear jar was last emptied by hand. */
  swearEmptied: string | null;
  /** Stored without `last_run` / `pending_count`; those are derived on read. */
  tasks: Task[];
  taskRuns: TaskRun[];
  sttModels: SttModel[];
  permissions: Permissions;
  /** Providers with a key "in the keychain". The key itself is never kept. */
  providerKeys: ProviderKind[];
  recording: RecordingState;
}

/** What every mock module receives: the state, a way to emit events, and a timer registry. */
export interface MockContext {
  state: MockState;
  emit(event: BackendEvent): void;
}

export const DEFAULT_MEETING_APPS = [
  "Zoom",
  "Microsoft Teams",
  "Google Meet",
  "Slack",
  "WhatsApp",
  "FaceTime",
  "Webex",
  "Discord",
  "Around",
];

export function defaultSettings(onboardingDone: boolean): Settings {
  return {
    appearance: {
      theme: "system",
      transparency: true,
      note_font: "sans",
      scene_headings: true,
      cilica_placement: "side_panel",
      home_face: "random",
    },
    recording: {
      auto_detect: true,
      detect_mode: "ask_first",
      meeting_apps: [...DEFAULT_MEETING_APPS],
      input_device_id: null,
      audio_retention_days: 30,
      consent_notice_seen: false,
    },
    transcription: {
      engine: "whisper_local",
      model_id: "small",
      language: null,
      refine_after_meeting: true,
      live_speaker_labels: true,
      identify_speakers_after_meeting: true,
      recognise_voices: true,
      cloud: { base_url: null, model: null },
    },
    ai: {
      provider: "anthropic",
      model: "claude-sonnet-5-5",
      fast_model: "claude-haiku-5-5",
      base_url: null,
      auto_apply_edits: false,
      conversation_retention_days: 7,
    },
    sounds: {
      enabled: true,
      recording_start_stop: true,
      notes_ready: true,
      notes_ready_sound: "chime",
      interface_sounds: false,
    },
    swear_jar: {
      enabled: true,
      transcripts: true,
      notes: true,
      mild: true,
      popup: true,
      price_cents: 100,
      clink: false,
    },
    writing: { check_spelling: true },
    keybinds: {},
    launch_at_login: false,
    onboarding_done: onboardingDone,
    updates: { auto_install: true },
  };
}

export function defaultSttModels(): SttModel[] {
  return [
    {
      id: "base",
      name: "Base",
      description: "Small and quick. Good for older or slower computers.",
      size_mb: 148,
      multilingual: true,
      recommended: false,
      downloaded: false,
    },
    {
      id: "small",
      name: "Small",
      description: "Better accuracy than Base at a moderate size.",
      size_mb: 488,
      multilingual: true,
      recommended: false,
      downloaded: true,
    },
    {
      id: "large-v3-turbo-q5_0",
      name: "Large v3 Turbo",
      description: "Most accurate, in many languages. Needs Apple silicon or a recent PC.",
      size_mb: 574,
      multilingual: true,
      recommended: true,
      downloaded: false,
    },
  ];
}

export function idleRecording(): RecordingState {
  return {
    phase: "idle",
    note_id: null,
    meeting_app: null,
    started_at: null,
    auto_stop_at: null,
    live_speakers: false,
  };
}
