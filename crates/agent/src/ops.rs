//! Write operations: what each write does, how it is described, how it becomes a proposal, how it
//! is applied (always versioning first) and how it is undone.

use std::collections::BTreeSet;

use cilica_domain::{
    AppError, AppResult, Author, DataScope, ErrorCode, Id, MoveGroup, NoteRef, ProposalKind, TaskDraft, ToolTier,
};
use cilica_store::{Db, NewNote};
use serde::{Deserialize, Serialize};

use crate::patch;
use crate::store::NoteStore;

/// A write the assistant wants to make, with everything resolved (titles, current values).
#[derive(Debug, Clone, PartialEq)]
pub enum WriteOp {
    CreateNote { title: String, body_md: String, category: Option<(Id, String)> },
    CreateCategory { name: String },
    MoveNotes { notes: Vec<NoteRef>, category: Option<(Id, String)> },
    /// Several bulk moves confirmed together (a grouped card); applied as one step with one undo.
    MoveGroups { groups: Vec<MoveGroup> },
    RenameCategory { category_id: Id, old_name: String, new_name: String },
    LabelSpeaker { note_id: Id, speaker_id: Id, old_label: String, label: Option<String>, person: Option<(Id, String)> },
    CreateTask { draft: TaskDraft },
    EditNote { note: NoteRef, before_md: String, after_md: String },
    RenameNote { note: NoteRef, new_title: String },
    TrashNotes { notes: Vec<NoteRef> },
    MergeNotes { sources: Vec<NoteRef>, merged_title: String, merged_md: String },
    DeleteCategory { category_id: Id, name: String, note_count: u32 },
    DeletePerson { person_id: Id, name: String },
}

/// How to reverse an applied write. Stored as `ai_actions.undo_json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum UndoOp {
    TrashCreatedNote { note_id: Id },
    DeleteCreatedCategory { category_id: Id },
    MoveBack { moves: Vec<(Id, Option<Id>, f64)> },
    RenameCategory { category_id: Id, name: String },
    RestoreSpeaker { speaker_id: Id, label: String, person_id: Option<Id> },
    DeleteCreatedTask { task_id: Id },
    RestoreBody { note_id: Id, before_md: String, after_md: String },
    RenameNote { note_id: Id, title: String, expected_title: String },
    RestoreNotes { note_ids: Vec<Id> },
    UnMerge { merged_note_id: Id, source_ids: Vec<Id> },
    RecreateCategory { name: String, position: f64, note_ids: Vec<Id> },
}

fn conflict(msg: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::Conflict, msg)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

fn notes_phrase(notes: &[NoteRef]) -> String {
    match notes {
        [one] => one.title_or_untitled(),
        _ => plural(notes.len(), "note", "notes"),
    }
}

/// "214 notes into 9 categories", or "12 notes to Clients" when there is one destination.
fn groups_phrase(groups: &[MoveGroup]) -> String {
    let count: usize = groups.iter().map(|g| g.notes.len()).sum();
    match groups {
        [one] if one.category_id.is_some() => format!("{} to {}", plural(count, "note", "notes"), one.category_name),
        [_] => format!("{} out of their category", plural(count, "note", "notes")),
        _ => format!("{} into {} categories", plural(count, "note", "notes"), groups.len()),
    }
}

/// Add a move to a grouped card. A note that was already headed somewhere else in the card now
/// goes to the new place (the latest call wins); groups left empty are dropped.
pub fn merge_move(groups: &mut Vec<MoveGroup>, notes: &[NoteRef], category: &Option<(Id, String)>) {
    let ids: BTreeSet<Id> = notes.iter().map(|n| n.id).collect();
    for g in groups.iter_mut() {
        g.notes.retain(|n| !ids.contains(&n.id));
    }
    groups.retain(|g| !g.notes.is_empty());
    let target = category.as_ref().map(|c| c.0);
    match groups.iter_mut().find(|g| g.category_id == target) {
        Some(g) => g.notes.extend(notes.iter().cloned()),
        None => groups.push(MoveGroup {
            notes: notes.to_vec(),
            category_id: target,
            category_name: category.as_ref().map(|c| c.1.clone()).unwrap_or_default(),
        }),
    }
}

trait TitleOrUntitled {
    fn title_or_untitled(&self) -> String;
}
impl TitleOrUntitled for NoteRef {
    fn title_or_untitled(&self) -> String {
        if self.title.trim().is_empty() { "Untitled".into() } else { self.title.clone() }
    }
}

impl WriteOp {
    pub fn base_tier(&self) -> ToolTier {
        match self {
            Self::CreateNote { .. }
            | Self::CreateCategory { .. }
            | Self::MoveNotes { .. }
            | Self::MoveGroups { .. }
            | Self::RenameCategory { .. }
            | Self::LabelSpeaker { .. }
            | Self::CreateTask { .. } => ToolTier::Additive,
            Self::EditNote { .. } | Self::RenameNote { .. } => ToolTier::Edit,
            Self::TrashNotes { .. } | Self::MergeNotes { .. } | Self::DeleteCategory { .. } | Self::DeletePerson { .. } => {
                ToolTier::Destructive
            }
        }
    }

    /// Existing notes this write changes (for the "more than 5 notes" rule).
    pub fn touched_notes(&self) -> Vec<Id> {
        match self {
            Self::MoveNotes { notes, .. } | Self::TrashNotes { notes } => notes.iter().map(|n| n.id).collect(),
            Self::MergeNotes { sources, .. } => sources.iter().map(|n| n.id).collect(),
            Self::MoveGroups { groups } => groups.iter().flat_map(|g| g.notes.iter().map(|n| n.id)).collect(),
            Self::EditNote { note, .. } | Self::RenameNote { note, .. } => vec![note.id],
            _ => vec![],
        }
    }

    /// Past tense, for the tool row and toasts.
    pub fn applied_summary(&self) -> String {
        match self {
            Self::CreateNote { title, .. } => format!("Created {title}"),
            Self::CreateCategory { name } => format!("Created the category {name}"),
            Self::MoveNotes { notes, category: Some((_, name)) } => format!("Moved {} to {name}", notes_phrase(notes)),
            Self::MoveNotes { notes, category: None } => format!("Moved {} out of its category", notes_phrase(notes)),
            Self::MoveGroups { groups } => format!("Moved {}", groups_phrase(groups)),
            Self::RenameCategory { old_name, new_name, .. } => format!("Renamed the category {old_name} to {new_name}"),
            Self::LabelSpeaker { old_label, label, person, .. } => match (label, person) {
                (_, Some((_, name))) => format!("Linked {old_label} to {name}"),
                (Some(l), None) => format!("Renamed {old_label} to {l}"),
                (None, None) => format!("Labelled {old_label}"),
            },
            Self::CreateTask { draft } => format!("Created the task {}", draft.name),
            Self::EditNote { note, .. } => format!("Edited {}", note.title_or_untitled()),
            Self::RenameNote { note, new_title } => format!("Renamed {} to {new_title}", note.title_or_untitled()),
            Self::TrashNotes { notes } => format!("Moved {} to Trash", notes_phrase(notes)),
            Self::MergeNotes { sources, merged_title, .. } => format!("Merged {} into {merged_title}", plural(sources.len(), "note", "notes")),
            Self::DeleteCategory { name, .. } => format!("Deleted the category {name}"),
            Self::DeletePerson { name, .. } => format!("Deleted {name} from Saved people"),
        }
    }

    /// Past tense, for a call that became a proposal.
    pub fn proposed_summary(&self) -> String {
        match self {
            Self::CreateNote { title, .. } => format!("Suggested a new note, {title}"),
            Self::MoveNotes { notes, category: Some((_, name)) } => format!("Asked to move {} to {name}", notes_phrase(notes)),
            Self::MoveNotes { notes, category: None } => format!("Asked to move {} out of their category", notes_phrase(notes)),
            Self::MoveGroups { groups } => format!("Asked to move {}", groups_phrase(groups)),
            Self::EditNote { note, .. } => format!("Suggested an edit to {}", note.title_or_untitled()),
            Self::RenameNote { note, new_title } => format!("Suggested renaming {} to {new_title}", note.title_or_untitled()),
            Self::TrashNotes { notes } => format!("Asked to move {} to Trash", notes_phrase(notes)),
            Self::MergeNotes { sources, merged_title, .. } => format!("Asked to merge {} into {merged_title}", plural(sources.len(), "note", "notes")),
            Self::DeleteCategory { name, .. } => format!("Asked to delete the category {name}"),
            Self::DeletePerson { name, .. } => format!("Asked to delete {name} from Saved people"),
            other => format!("Suggested: {}", other.applied_summary()),
        }
    }

    pub fn scopes(&self) -> (Vec<DataScope>, Option<Id>) {
        use DataScope::*;
        match self {
            Self::CreateNote { .. } => (vec![Notes], None),
            Self::CreateCategory { .. } | Self::RenameCategory { .. } => (vec![Categories], None),
            Self::MoveNotes { .. } | Self::MoveGroups { .. } => (vec![Notes, Categories], None),
            Self::LabelSpeaker { note_id, .. } => (vec![Transcript, People], Some(*note_id)),
            Self::CreateTask { .. } => (vec![Tasks], None),
            Self::EditNote { note, .. } | Self::RenameNote { note, .. } => (vec![Notes, Versions], Some(note.id)),
            Self::TrashNotes { .. } | Self::MergeNotes { .. } => (vec![Notes, Trash], None),
            Self::DeleteCategory { .. } => (vec![Categories, Notes], None),
            Self::DeletePerson { .. } => (vec![People, Transcript], None),
        }
    }

    /// The proposal card for this write, when it has one.
    pub fn to_proposal_kind(&self) -> Option<ProposalKind> {
        Some(match self.clone() {
            Self::CreateNote { title, body_md, category } => ProposalKind::CreateNote {
                title,
                body_md,
                category_id: category.as_ref().map(|c| c.0),
                category_name: category.map(|c| c.1),
            },
            Self::MoveNotes { notes, category } => ProposalKind::BulkMove {
                notes,
                category_id: category.as_ref().map(|c| c.0),
                category_name: category.map(|c| c.1).unwrap_or_default(),
            },
            Self::MoveGroups { groups } => ProposalKind::MoveGroups { groups },
            Self::EditNote { note, before_md, after_md } => ProposalKind::EditNote { note, before_md, after_md },
            Self::RenameNote { note, new_title } => ProposalKind::RenameNote { note, new_title },
            Self::TrashNotes { notes } => ProposalKind::TrashNotes { notes },
            Self::MergeNotes { sources, merged_title, merged_md } => ProposalKind::MergeNotes { sources, merged_title, merged_md },
            Self::DeleteCategory { category_id, name, note_count } => ProposalKind::DeleteCategory { category_id, name, note_count },
            Self::DeletePerson { person_id, name } => ProposalKind::DeletePerson { person_id, name },
            _ => return None,
        })
    }

    /// The write a proposal stands for. For an edit, `final_md` replaces the proposed text
    /// (the user accepted only some hunks).
    pub fn from_proposal_kind(kind: &ProposalKind, final_md: Option<String>) -> Self {
        match kind.clone() {
            ProposalKind::EditNote { note, before_md, after_md } => Self::EditNote { note, before_md, after_md: final_md.unwrap_or(after_md) },
            ProposalKind::RenameNote { note, new_title } => Self::RenameNote { note, new_title },
            ProposalKind::TrashNotes { notes } => Self::TrashNotes { notes },
            ProposalKind::MergeNotes { sources, merged_title, merged_md } => {
                Self::MergeNotes { sources, merged_title, merged_md: final_md.unwrap_or(merged_md) }
            }
            ProposalKind::DeleteCategory { category_id, name, note_count } => Self::DeleteCategory { category_id, name, note_count },
            ProposalKind::DeletePerson { person_id, name } => Self::DeletePerson { person_id, name },
            ProposalKind::BulkMove { notes, category_id, category_name } => {
                Self::MoveNotes { notes, category: category_id.map(|id| (id, category_name)) }
            }
            ProposalKind::CreateNote { title, body_md, category_id, category_name } => Self::CreateNote {
                title,
                body_md: final_md.unwrap_or(body_md),
                category: category_id.map(|id| (id, category_name.unwrap_or_default())),
            },
            ProposalKind::MoveGroups { groups } => Self::MoveGroups { groups },
        }
    }
}

/// The tool name a proposal kind stands for (for audit rows made at apply time).
pub fn tool_for_kind(kind: &ProposalKind) -> &'static str {
    match kind {
        ProposalKind::EditNote { .. } => "edit_note",
        ProposalKind::RenameNote { .. } => "rename_note",
        ProposalKind::TrashNotes { .. } => "trash_note",
        ProposalKind::MergeNotes { .. } => "merge_notes",
        ProposalKind::DeleteCategory { .. } => "delete_category",
        ProposalKind::DeletePerson { .. } => "delete_person",
        ProposalKind::BulkMove { .. } | ProposalKind::MoveGroups { .. } => "move_note",
        ProposalKind::CreateNote { .. } => "create_note",
    }
}

// ── Fingerprints ─────────────────────────────────────────────────────────

/// FNV-1a 64, stable across runs and builds.
fn fnv(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn sorted_ids(notes: &[NoteRef]) -> String {
    notes.iter().map(|n| n.id.to_string()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(",")
}

/// A stable identity for "the same suggestion", so a dismissed one is not made again. Edits key on
/// the note and its body at the time, so a new suggestion is possible once the note changes.
pub fn fingerprint(kind: &ProposalKind) -> String {
    let raw = match kind {
        ProposalKind::EditNote { note, before_md, .. } => format!("edit:{}:{}", note.id, fnv(before_md)),
        ProposalKind::RenameNote { note, new_title } => format!("rename:{}:{}", note.id, fnv(&new_title.to_lowercase())),
        ProposalKind::TrashNotes { notes } => format!("trash:{}", sorted_ids(notes)),
        ProposalKind::MergeNotes { sources, .. } => format!("merge:{}", sorted_ids(sources)),
        ProposalKind::DeleteCategory { category_id, .. } => format!("delete_category:{category_id}"),
        ProposalKind::DeletePerson { person_id, .. } => format!("delete_person:{person_id}"),
        ProposalKind::BulkMove { notes, category_id, .. } => {
            format!("move:{}:{}", sorted_ids(notes), category_id.map(|c| c.to_string()).unwrap_or_default())
        }
        ProposalKind::CreateNote { title, body_md, .. } => format!("create:{}", fnv(&format!("{}\n{body_md}", title.to_lowercase()))),
        ProposalKind::MoveGroups { groups } => {
            let parts: BTreeSet<String> = groups
                .iter()
                .map(|g| format!("{}>{}", sorted_ids(&g.notes), g.category_id.map(|c| c.to_string()).unwrap_or_default()))
                .collect();
            format!("move_groups:{}", parts.into_iter().collect::<Vec<_>>().join(";"))
        }
    };
    fnv(&raw)
}

// ── Apply and undo ───────────────────────────────────────────────────────

/// Position that puts a note at the top of `category_id`.
async fn top_position(notes: &dyn NoteStore, category_id: Option<Id>) -> AppResult<f64> {
    let list = notes.list_notes().await?;
    let min = list.iter().filter(|n| n.category_id == category_id).map(|n| n.position).fold(f64::INFINITY, f64::min);
    Ok(if min.is_finite() { min - 1.0 } else { 0.0 })
}

/// Apply a write. AI-authored body changes snapshot a version tagged with `action_id` first.
pub async fn apply(notes: &dyn NoteStore, db: &Db, op: &WriteOp, action_id: Id) -> AppResult<Option<UndoOp>> {
    Ok(match op {
        WriteOp::CreateNote { title, body_md, category } => {
            let note = notes
                .create_note(NewNote { title: title.clone(), body_md: body_md.clone(), category_id: category.as_ref().map(|c| c.0), ..Default::default() })
                .await?;
            Some(UndoOp::TrashCreatedNote { note_id: note.id })
        }
        WriteOp::CreateCategory { name } => {
            let c = notes.create_category(name).await?;
            Some(UndoOp::DeleteCreatedCategory { category_id: c.id })
        }
        WriteOp::MoveNotes { notes: refs, category } => {
            let target = category.as_ref().map(|c| c.0);
            let mut moves = Vec::new();
            let mut position = top_position(notes, target).await?;
            for r in refs {
                let n = notes.get_note(r.id).await?;
                notes.move_note(r.id, target, position).await?;
                position -= 1.0;
                moves.push((r.id, n.category_id, n.position));
            }
            Some(UndoOp::MoveBack { moves })
        }
        WriteOp::MoveGroups { groups } => {
            // One undo for the whole card: every note goes back to where it was.
            let mut moves = Vec::new();
            for g in groups {
                let target = g.category_id;
                let mut position = top_position(notes, target).await?;
                for r in &g.notes {
                    let n = notes.get_note(r.id).await?;
                    notes.move_note(r.id, target, position).await?;
                    position -= 1.0;
                    moves.push((r.id, n.category_id, n.position));
                }
            }
            Some(UndoOp::MoveBack { moves })
        }
        WriteOp::RenameCategory { category_id, old_name, new_name } => {
            notes.rename_category(*category_id, new_name).await?;
            Some(UndoOp::RenameCategory { category_id: *category_id, name: old_name.clone() })
        }
        WriteOp::LabelSpeaker { note_id, speaker_id, label, person, .. } => {
            let before = notes
                .list_speakers(*note_id)
                .await?
                .into_iter()
                .find(|s| s.id == *speaker_id)
                .ok_or_else(|| AppError::not_found("That speaker was not found."))?;
            if let Some((pid, _)) = person {
                notes.link_speaker(*speaker_id, Some(*pid)).await?;
            }
            if let Some(l) = label {
                notes.rename_speaker(*speaker_id, l).await?;
            }
            Some(UndoOp::RestoreSpeaker { speaker_id: *speaker_id, label: before.label, person_id: before.person_id })
        }
        WriteOp::CreateTask { draft } => {
            let task = db.insert_task(draft, false, None).await?;
            Some(UndoOp::DeleteCreatedTask { task_id: task.id })
        }
        WriteOp::EditNote { note, before_md, after_md } => {
            let current = notes.get_note(note.id).await?;
            if current.deleted_at.is_some() {
                return Err(conflict(format!("{} is in Trash, so the edit was not applied.", note.title_or_untitled())));
            }
            let new_body = patch::rebase(before_md, after_md, &current.body_md).ok_or_else(|| {
                conflict(format!(
                    "{} changed since Cilica suggested this edit, and the two changes overlap. Ask Cilica again.",
                    note.title_or_untitled()
                ))
            })?;
            notes.set_note_body(note.id, &new_body, Author::Ai, Some(action_id)).await?;
            Some(UndoOp::RestoreBody { note_id: note.id, before_md: current.body_md, after_md: new_body })
        }
        WriteOp::RenameNote { note, new_title } => {
            let current = notes.get_note(note.id).await?;
            notes.snapshot_note(note.id, Author::Ai, Some(action_id)).await?;
            notes.rename_note(note.id, new_title).await?;
            Some(UndoOp::RenameNote { note_id: note.id, title: current.title, expected_title: new_title.clone() })
        }
        WriteOp::TrashNotes { notes: refs } => {
            let mut trashed = Vec::new();
            for r in refs {
                let n = notes.get_note(r.id).await?;
                if n.deleted_at.is_none() {
                    notes.trash_note(r.id).await?;
                    trashed.push(r.id);
                }
            }
            Some(UndoOp::RestoreNotes { note_ids: trashed })
        }
        WriteOp::MergeNotes { sources, merged_title, merged_md } => {
            let mut category = None;
            for (i, r) in sources.iter().enumerate() {
                let n = notes.get_note(r.id).await?;
                if n.deleted_at.is_some() {
                    return Err(conflict(format!("{} is already in Trash, so the merge was not applied.", r.title_or_untitled())));
                }
                if i == 0 {
                    category = n.category_id;
                }
            }
            let merged = notes
                .create_note(NewNote { title: merged_title.clone(), body_md: merged_md.clone(), category_id: category, ..Default::default() })
                .await?;
            for r in sources {
                notes.trash_note(r.id).await?;
            }
            Some(UndoOp::UnMerge { merged_note_id: merged.id, source_ids: sources.iter().map(|s| s.id).collect() })
        }
        WriteOp::DeleteCategory { category_id, .. } => {
            let c = notes.get_category(*category_id).await?;
            let members: Vec<Id> =
                notes.list_notes().await?.into_iter().filter(|n| n.category_id == Some(*category_id)).map(|n| n.id).collect();
            notes.delete_category(*category_id).await?;
            Some(UndoOp::RecreateCategory { name: c.name, position: c.position, note_ids: members })
        }
        WriteOp::DeletePerson { person_id, .. } => {
            notes.delete_person(*person_id).await?;
            None // The voiceprint cannot be rebuilt.
        }
    })
}

/// Reverse an applied write. Refuses when the user changed the same thing since.
pub async fn undo(notes: &dyn NoteStore, db: &Db, op: &UndoOp, action_id: Id) -> AppResult<()> {
    match op {
        UndoOp::TrashCreatedNote { note_id } => notes.trash_note(*note_id).await?,
        UndoOp::DeleteCreatedCategory { category_id } => notes.delete_category(*category_id).await?,
        UndoOp::MoveBack { moves } => {
            for (id, cat, pos) in moves {
                notes.move_note(*id, *cat, *pos).await?;
            }
        }
        UndoOp::RenameCategory { category_id, name } => {
            notes.rename_category(*category_id, name).await?;
        }
        UndoOp::RestoreSpeaker { speaker_id, label, person_id } => {
            notes.link_speaker(*speaker_id, *person_id).await?;
            notes.rename_speaker(*speaker_id, label).await?;
        }
        UndoOp::DeleteCreatedTask { task_id } => match db.delete_task(*task_id).await {
            Ok(()) | Err(cilica_store::StoreError::NotFound(_)) => {}
            Err(e) => return Err(e.into()),
        },
        UndoOp::RestoreBody { note_id, before_md, after_md } => {
            let current = notes.get_note(*note_id).await?;
            if current.body_md != *after_md {
                return Err(conflict(
                    "This note changed after Cilica's edit, so Undo would overwrite your changes. Restore an earlier version from History instead.",
                ));
            }
            // Keep the AI-written state in history, then put the old body back as the user.
            notes.snapshot_note(*note_id, Author::Ai, Some(action_id)).await?;
            notes.set_note_body(*note_id, before_md, Author::User, None).await?;
        }
        UndoOp::RenameNote { note_id, title, expected_title } => {
            let current = notes.get_note(*note_id).await?;
            if current.title != *expected_title {
                return Err(conflict("This note was renamed again since, so Undo left it as it is."));
            }
            notes.rename_note(*note_id, title).await?;
        }
        UndoOp::RestoreNotes { note_ids } => {
            for id in note_ids {
                notes.restore_note(*id).await?;
            }
        }
        UndoOp::UnMerge { merged_note_id, source_ids } => {
            for id in source_ids {
                notes.restore_note(*id).await?;
            }
            notes.trash_note(*merged_note_id).await?;
        }
        UndoOp::RecreateCategory { name, position, note_ids } => {
            let c = notes.create_category(name).await?;
            notes.reorder_category(c.id, *position).await?;
            let current = notes.list_notes().await?;
            let mut pos = 0.0;
            for id in note_ids {
                // Only notes the user has not filed elsewhere since.
                if current.iter().any(|n| n.id == *id && n.category_id.is_none()) {
                    notes.move_note(*id, Some(c.id), pos).await?;
                    pos += 1.0;
                }
            }
        }
    }
    Ok(())
}

impl UndoOp {
    pub fn scopes(&self) -> (Vec<DataScope>, Option<Id>) {
        use DataScope::*;
        match self {
            Self::TrashCreatedNote { .. } | Self::RestoreNotes { .. } | Self::UnMerge { .. } => (vec![Notes, Trash], None),
            Self::DeleteCreatedCategory { .. } | Self::RecreateCategory { .. } => (vec![Categories, Notes], None),
            Self::MoveBack { .. } => (vec![Notes, Categories], None),
            Self::RenameCategory { .. } => (vec![Categories], None),
            Self::RestoreSpeaker { .. } => (vec![Transcript, People], None),
            Self::DeleteCreatedTask { .. } => (vec![Tasks], None),
            Self::RestoreBody { note_id, .. } | Self::RenameNote { note_id, .. } => (vec![Notes, Versions], Some(*note_id)),
        }
    }
}
