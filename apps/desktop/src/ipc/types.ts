/**
 * Tidy names for the generated IPC types.
 *
 * tauri-specta splits structs with optional fields into `X_Serialize` (what Rust sends, every
 * field present) and `X_Deserialize` (what Rust accepts, optional fields may be omitted).
 * Features import the names below: data coming from the backend uses the Serialize shape,
 * arguments going to it use the Deserialize shape.
 */
import type {
  AiAction_Serialize,
  AiSettings_Serialize,
  AppEvent_Serialize,
  AppearanceSettings_Serialize,
  AssistantEvent_Serialize,
  Backlinks,
  ClaudeCodeStatus_Serialize,
  CloudSttSettings_Serialize,
  commands,
  GraphNode_Serialize,
  HomeFace_Serialize,
  HomeGlance_Serialize,
  ImportDetection_Serialize,
  ImportInput_Deserialize,
  ImportKeyStatus_Serialize,
  ImportPreview_Serialize,
  ImportProgress_Serialize,
  ImportRun_Serialize,
  LinkGraph_Serialize,
  MeetingDetected_Serialize,
  Message_Serialize,
  MessagePart_Serialize,
  ModelDownloadProgress_Serialize,
  NearWord_Serialize,
  Note_Serialize,
  NoteQuery_Deserialize,
  NoteSummary_Serialize,
  NoteVersion_Serialize,
  OllamaStatus_Serialize,
  Overheard_Serialize,
  Pace_Serialize,
  Proposal_Serialize,
  ProposalKind_Serialize,
  ProviderStatus_Serialize,
  RecordingSettings_Serialize,
  RecordingState_Serialize,
  SearchHit_Serialize,
  Segment_Serialize,
  Settings_Deserialize,
  Settings_Serialize,
  Silence_Serialize,
  SoundSettings_Serialize,
  Speaker_Serialize,
  StartRecordingOptions_Deserialize,
  SttProviderStatus_Serialize,
  SwearJar_Serialize,
  SwearJarSettings_Serialize,
  SwearPerson_Serialize,
  TalkShare_Serialize,
  TalkSlice_Serialize,
  Task_Serialize,
  TaskRun_Serialize,
  TaskRunDetail_Serialize,
  ToastAction_Serialize,
  ToastRequest_Serialize,
  Transcript_Serialize,
  TranscriptionSettings_Serialize,
  TranscriptionTest_Serialize,
  UpdateInfo_Serialize,
  UpdateSettings_Serialize,
  ViewContext_Deserialize,
} from "./bindings";

export type {
  AiActionStatus,
  AppError,
  AppInfo,
  AudioChannel,
  AudioDevice,
  AudioLevels,
  Author,
  Category,
  CilicaPlacement,
  DataScope,
  DateField,
  DetectedLocation,
  DetectMode,
  ErrorCode,
  GraphEdge,
  GraphEdgeKind,
  GraphNodeKind,
  HapticKind,
  HardwareProfile,
  ImportCategoryMode,
  ImportFidelity,
  ImportOptions,
  ImportPhase,
  ImportRunStatus,
  ImportSample,
  IslandInfo,
  LocalLlmModel,
  Mention,
  NoteFont,
  NoteKind,
  NoteLine,
  NotePanel,
  NoteRef,
  PermissionKind,
  PermissionStatus,
  Permissions,
  Person,
  PromptChip,
  ProposalSource,
  ProposalStatus,
  ProviderKind,
  Quote,
  ReadySound,
  RecordingPhase,
  RecordingTrigger,
  Role,
  SearchFacets,
  SearchSource,
  SpeakerCount,
  SpeakerSuggestion,
  SttEngine,
  SttModel,
  SwearCount,
  SwearWeek,
  SwearWord,
  TaskDraft,
  TaskRunStatus,
  TaskSchedule,
  TaskScope,
  ThemeMode,
  Thread,
  ThreadHit,
  TimeOfDay,
  ToastKind,
  ToolTier,
  TrashScope,
  TurnEnd,
  UnsupportedItem,
  WritingHours,
} from "./bindings";

/** The full command surface. The mock implements exactly this shape. */
export type Commands = typeof commands;

export type Note = Note_Serialize;
export type LinkGraph = LinkGraph_Serialize;
export type GraphNode = GraphNode_Serialize;
export type { Backlinks };
export type NoteSummary = NoteSummary_Serialize;
export type NoteVersion = NoteVersion_Serialize;
export type SearchHit = SearchHit_Serialize;
/** A structured search, as `findNotes` accepts it. */
export type NoteQuery = NoteQuery_Deserialize;

export type Settings = Settings_Serialize;
/** What `updateSettings` accepts: every section optional. */
export type SettingsInput = Settings_Deserialize;
export type AppearanceSettings = AppearanceSettings_Serialize;
export type RecordingSettings = RecordingSettings_Serialize;
export type TranscriptionSettings = TranscriptionSettings_Serialize;
export type CloudSttSettings = CloudSttSettings_Serialize;
export type SttProviderStatus = SttProviderStatus_Serialize;
export type TranscriptionTest = TranscriptionTest_Serialize;
export type AiSettings = AiSettings_Serialize;
export type SoundSettings = SoundSettings_Serialize;
export type SwearJarSettings = SwearJarSettings_Serialize;
export type UpdateSettings = UpdateSettings_Serialize;
export type UpdateInfo = UpdateInfo_Serialize;

export type RecordingState = RecordingState_Serialize;
export type StartRecordingOptions = StartRecordingOptions_Deserialize;
export type Transcript = Transcript_Serialize;
export type HomeGlance = HomeGlance_Serialize;
/** A kept face as Rust sends it; an old "word" is read as "tics" on the way in. */
export type HomeFace = HomeFace_Serialize;
export type Overheard = Overheard_Serialize;
export type NearWord = NearWord_Serialize;
export type TalkShare = TalkShare_Serialize;
export type TalkSlice = TalkSlice_Serialize;
export type Pace = Pace_Serialize;
export type Silence = Silence_Serialize;
export type SwearJar = SwearJar_Serialize;
export type SwearPerson = SwearPerson_Serialize;
export type Segment = Segment_Serialize;
export type Speaker = Speaker_Serialize;
export type MeetingDetected = MeetingDetected_Serialize;
export type ModelDownloadProgress = ModelDownloadProgress_Serialize;

export type Message = Message_Serialize;
export type MessagePart = MessagePart_Serialize;
export type AiAction = AiAction_Serialize;
export type Proposal = Proposal_Serialize;
export type ProposalKind = ProposalKind_Serialize;
export type ProviderStatus = ProviderStatus_Serialize;
export type ClaudeCodeStatus = ClaudeCodeStatus_Serialize;
export type OllamaStatus = OllamaStatus_Serialize;
export type AssistantEvent = AssistantEvent_Serialize;
export type ViewContext = ViewContext_Deserialize;

export type Task = Task_Serialize;
export type TaskRun = TaskRun_Serialize;
export type TaskRunDetail = TaskRunDetail_Serialize;

export type ImportDetection = ImportDetection_Serialize;
export type ImportInput = ImportInput_Deserialize;
export type ImportKeyStatus = ImportKeyStatus_Serialize;
export type ImportPreview = ImportPreview_Serialize;
export type ImportProgress = ImportProgress_Serialize;
export type ImportRun = ImportRun_Serialize;

export type ToastRequest = ToastRequest_Serialize;
export type ToastAction = ToastAction_Serialize;

/** The single backend → UI event, as the UI receives it. */
export type BackendEvent = AppEvent_Serialize;
export type BackendEventType = BackendEvent["type"];
/** Narrow a backend event by its `type`. */
export type BackendEventOf<T extends BackendEventType> = Extract<BackendEvent, { type: T }>;
