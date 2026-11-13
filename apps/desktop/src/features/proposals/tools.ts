/** Icons for the assistant's tools and proposal kinds, shared by the thread and the activity log. */
import {
  AudioLines,
  CalendarPlus,
  FilePlus,
  FileText,
  FolderInput,
  FolderPlus,
  History,
  List,
  type LucideIcon,
  Merge,
  PencilLine,
  PenLine,
  Search,
  Trash2,
  UserRound,
  UserX,
  Wrench,
} from "lucide-react";
import type { ProposalKind, ToolTier } from "@/ipc/types";

const TOOL_ICONS: Record<string, LucideIcon> = {
  search_notes: Search,
  get_note: FileText,
  get_transcript: AudioLines,
  list_categories: List,
  list_people: List,
  list_tasks: List,
  get_note_history: History,
  create_note: FilePlus,
  create_category: FolderPlus,
  move_note: FolderInput,
  rename_category: PenLine,
  label_speaker: UserRound,
  create_task: CalendarPlus,
  edit_note: PencilLine,
  rename_note: PenLine,
  trash_note: Trash2,
  trash_notes: Trash2,
  merge_notes: Merge,
  delete_category: Trash2,
  delete_person: UserX,
  bulk_move: FolderInput,
  move_groups: FolderInput,
};

export function toolIcon(tool: string): LucideIcon {
  return TOOL_ICONS[tool] ?? Wrench;
}

export function kindIcon(kind: ProposalKind["type"]): LucideIcon {
  return TOOL_ICONS[kind] ?? Wrench;
}

/** Plain words for a permission tier. */
export const TIER_LABEL: Record<ToolTier, string> = {
  read: "Read",
  additive: "Add",
  edit: "Edit",
  destructive: "Destructive",
};

/** Tool names as people read them ("search_notes" → "Search notes"). */
export function toolName(tool: string): string {
  const words = tool.replace(/_/g, " ").trim();
  return words ? words[0]?.toUpperCase() + words.slice(1) : tool;
}
