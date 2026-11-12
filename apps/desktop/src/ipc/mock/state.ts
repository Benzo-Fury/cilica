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
  /** For undone note edits: the body before and after Margin's change (the real store keeps
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
      accent: "amber",
      transparency: true,
      note_font: "sans",
      margin_placement: "side_panel",
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
    transcription: { engine: "whisper_local", model_id: "small.en", language: null },
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
    keybinds: {},
    launch_at_login: false,
    onboarding_done: onboardingDone,
    updates: { auto_install: true },
  };
}

export function defaultSttModels(): SttModel[] {
  return [
    {
      id: "base.en",
      name: "Base (English)",
      description: "Fast and light. Good for clear audio on older Macs.",
      size_mb: 142,
      multilingual: false,
      recommended: false,
      downloaded: false,
    },
    {
      id: "small.en",
      name: "Small (English)",
      description: "The best balance of speed and accuracy for English meetings.",
      size_mb: 466,
      multilingual: false,
      recommended: true,
      downloaded: true,
    },
    {
      id: "small",
      name: "Small (multilingual)",
      description: "Transcribes 99 languages. A little slower than the English model.",
      size_mb: 466,
      multilingual: true,
      recommended: false,
      downloaded: false,
    },
    {
      id: "large-v3-turbo",
      name: "Large v3 Turbo",
      description: "Most accurate, multilingual. Needs Apple silicon or a recent GPU.",
      size_mb: 1550,
      multilingual: true,
      recommended: false,
      downloaded: false,
    },
  ];
}

export function idleRecording(): RecordingState {
  return { phase: "idle", note_id: null, meeting_app: null, started_at: null, auto_stop_at: null };
}
