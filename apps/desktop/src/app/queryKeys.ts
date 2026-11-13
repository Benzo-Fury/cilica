/**
 * TanStack Query keys, one family per data scope. `data_changed` events from the backend map
 * a `DataScope` to the keys to invalidate (`keysForScope`).
 */
import type { QueryKey } from "@tanstack/react-query";
import type { DataScope } from "@/ipc/types";

export const qk = {
  notes: ["notes"] as const,
  note: (id: string) => ["note", id] as const,
  notePrefix: ["note"] as const,
  categories: ["categories"] as const,
  trash: ["trash"] as const,
  versions: (noteId: string) => ["versions", noteId] as const,
  versionsPrefix: ["versions"] as const,
  search: (query: string) => ["search", query] as const,
  searchPrefix: ["search"] as const,
  nearWords: (query: string) => ["search", "near", query] as const,
  settings: ["settings"] as const,
  appInfo: ["appInfo"] as const,
  recording: ["recording"] as const,
  transcript: (noteId: string) => ["transcript", noteId] as const,
  transcriptPrefix: ["transcript"] as const,
  people: ["people"] as const,
  tasks: ["tasks"] as const,
  proposals: ["proposals"] as const,
  aiActions: ["ai_actions"] as const,
  threads: ["threads"] as const,
  threadMessages: (threadId: string) => ["thread_messages", threadId] as const,
  threadMessagesPrefix: ["thread_messages"] as const,
  proposal: (id: string) => ["proposals", id] as const,
  promptChips: (key: string) => ["prompt_chips", key] as const,
  promptChipsPrefix: ["prompt_chips"] as const,
  providerStatus: ["provider_status"] as const,
  claudeCode: ["claude_code"] as const,
  taskRuns: (taskId: string) => ["tasks", "runs", taskId] as const,
  taskRun: (runId: string) => ["tasks", "run", runId] as const,
  speakerSuggestions: (noteId: string) => ["speaker_suggestions", noteId] as const,
  speakerSuggestionsPrefix: ["speaker_suggestions"] as const,
  audioPath: (noteId: string) => ["audio_path", noteId] as const,
  audioPathPrefix: ["audio_path"] as const,
  sttModels: ["stt_models"] as const,
  hardwareProfile: ["hardware_profile"] as const,
  sttProviderStatus: (engine: string) => ["stt_provider_status", engine] as const,
  sttProviderStatusPrefix: ["stt_provider_status"] as const,
  localModels: ["local_models"] as const,
  ollamaStatus: ["ollama_status"] as const,
  permissions: ["permissions"] as const,
  inputDevices: ["input_devices"] as const,
  islandInfo: ["island_info"] as const,
  homeGlance: ["home_glance"] as const,
  swearJar: ["swear_jar"] as const,
  swearWords: ["swear_words"] as const,
  links: ["links"] as const,
  linkGraph: (focus: string | null, depth: number) =>
    ["links", "graph", focus ?? "all", depth] as const,
  backlinks: (noteId: string) => ["links", "backlinks", noteId] as const,
  imports: ["imports"] as const,
  importDetect: (provider: string) => ["imports", "detect", provider] as const,
  importDetectAll: ["imports", "detect_all"] as const,
};

/** The query keys a `data_changed` scope invalidates; `noteId` narrows per-note keys. */
export function keysForScope(scope: DataScope, noteId?: string | null): QueryKey[] {
  switch (scope) {
    case "notes":
      return [
        qk.notes,
        noteId ? qk.note(noteId) : qk.notePrefix,
        qk.searchPrefix,
        qk.homeGlance,
        qk.links,
      ];
    case "categories":
      return [qk.categories, qk.notes];
    case "trash":
      return [qk.trash, qk.links];
    case "versions":
      return [noteId ? qk.versions(noteId) : qk.versionsPrefix];
    case "transcript":
      // The audio file appears after Stop and goes once retention removes it.
      return [
        noteId ? qk.transcript(noteId) : qk.transcriptPrefix,
        noteId ? qk.speakerSuggestions(noteId) : qk.speakerSuggestionsPrefix,
        noteId ? qk.audioPath(noteId) : qk.audioPathPrefix,
        qk.searchPrefix,
        qk.homeGlance,
        // People heard in a meeting join it in the graph.
        qk.links,
      ];
    case "people":
      // A person's name or colour shows on every transcript speaker linked to them.
      return [qk.people, qk.transcriptPrefix, qk.speakerSuggestionsPrefix, qk.homeGlance, qk.links];
    case "tasks":
      // Task runs and reports live under the same prefix.
      return [qk.tasks];
    case "proposals":
      return [qk.proposals, qk.tasks];
    case "ai_actions":
      return [qk.aiActions];
    case "threads":
      return [qk.threads];
    case "swear_jar":
      return [qk.swearJar, qk.homeGlance];
    case "imports":
      return [qk.imports];
  }
}
