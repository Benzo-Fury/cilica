//! The tool registry: names, tiers, JSON schemas and descriptions, exactly as the spec's tool table.
//! Tiers here are the *base* tier; the runner can only raise a call's tier (never lower it).

use cilica_domain::ToolTier;
use cilica_llm::ToolSpec;
use serde_json::{Value, json};

/// One tool.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: &'static str,
    pub tier: ToolTier,
    pub description: &'static str,
    /// Shown on the tool row while it runs ("Searching notes").
    pub running: &'static str,
    /// Can run in a scheduled task, where every write must become a proposal.
    pub in_tasks: bool,
}

const NOTE_ID: &str = "Id of the note, from search_notes or the <view> block.";
const REASON: &str = "One or two plain sentences for the user saying why. Shown on the confirm or diff card.";

pub const TOOLS: &[ToolDef] = &[
    // ── Read ──
    ToolDef {
        name: "search_notes",
        tier: ToolTier::Read,
        description: "Full-text search over note titles, bodies and transcripts (notes in Trash are excluded unless asked). Narrow it by date, kind, person, tag, meeting app or category; with only filters it lists the notes that match, newest first. Without a query or filters, lists notes, most recently updated first. Returns ids, titles, dates and short excerpts.",
        running: "Searching notes",
        in_tasks: true,
    },
    ToolDef { name: "get_note", tier: ToolTier::Read, description: "Read one note: title, metadata and its full Markdown body.", running: "Reading a note", in_tasks: true },
    ToolDef {
        name: "get_transcript",
        tier: ToolTier::Read,
        description: "Read a meeting note's transcript with speaker labels and [mm:ss] timecodes, plus the speaker ids.",
        running: "Reading a transcript",
        in_tasks: true,
    },
    ToolDef { name: "list_categories", tier: ToolTier::Read, description: "List categories with ids and note counts.", running: "Listing categories", in_tasks: true },
    ToolDef { name: "list_people", tier: ToolTier::Read, description: "List Saved people (voiceprints) with ids and meeting counts.", running: "Listing people", in_tasks: true },
    ToolDef { name: "list_tasks", tier: ToolTier::Read, description: "List scheduled tasks with their schedules and next run.", running: "Listing tasks", in_tasks: true },
    ToolDef { name: "get_note_history", tier: ToolTier::Read, description: "List a note's saved versions (who wrote each and when), newest first.", running: "Reading history", in_tasks: true },
    // ── Additive ──
    ToolDef { name: "create_note", tier: ToolTier::Additive, description: "Create a new note with a title and a Markdown body.", running: "Creating a note", in_tasks: true },
    ToolDef { name: "create_category", tier: ToolTier::Additive, description: "Create a category.", running: "Creating a category", in_tasks: false },
    ToolDef {
        name: "move_note",
        tier: ToolTier::Additive,
        description: "Move one or more notes into a category, or out of every category with category_id null. Pass every note for one category in a single call. Moves past 5 notes in a turn gather on one card the user confirms once.",
        running: "Moving notes",
        in_tasks: true,
    },
    ToolDef { name: "rename_category", tier: ToolTier::Additive, description: "Rename a category.", running: "Renaming a category", in_tasks: false },
    ToolDef {
        name: "label_speaker",
        tier: ToolTier::Additive,
        description: "Name a speaker in a meeting transcript and/or link them to a Saved person. Get speaker ids from get_transcript.",
        running: "Labelling a speaker",
        in_tasks: false,
    },
    ToolDef { name: "create_task", tier: ToolTier::Additive, description: "Create a scheduled task that runs a prompt over the user's notes on a schedule. Its runs only suggest changes.", running: "Creating a task", in_tasks: false },
    // ── Edit ──
    ToolDef {
        name: "edit_note",
        tier: ToolTier::Edit,
        description: "Change part of a note with exact search/replace edits. Each `find` must be copied exactly from the current body (get_note first) and occur once; keep it short but unique. An empty `find` appends to the end. Never rewrite the whole note. The user reviews the change as a diff before it applies.",
        running: "Preparing an edit",
        in_tasks: true,
    },
    ToolDef { name: "rename_note", tier: ToolTier::Edit, description: "Change a note's title. The user confirms first.", running: "Preparing a rename", in_tasks: true },
    // ── Destructive ──
    ToolDef { name: "trash_note", tier: ToolTier::Destructive, description: "Move notes to Trash (kept 30 days). Always asks the user to confirm.", running: "Preparing to move notes to Trash", in_tasks: true },
    ToolDef {
        name: "merge_notes",
        tier: ToolTier::Destructive,
        description: "Combine 2 or more notes into one new note with the given title and Markdown body; the originals go to Trash. Always asks the user to confirm.",
        running: "Preparing a merge",
        in_tasks: true,
    },
    ToolDef { name: "delete_category", tier: ToolTier::Destructive, description: "Delete a category; its notes become uncategorised. Always asks the user to confirm.", running: "Preparing to delete a category", in_tasks: true },
    ToolDef { name: "delete_person", tier: ToolTier::Destructive, description: "Delete a Saved person and their voiceprint. Always asks the user to confirm.", running: "Preparing to delete a person", in_tasks: true },
];

pub fn find(name: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.name == name)
}

/// Tool names, for the MCP server and the Claude Code allow-list.
pub fn names() -> Vec<String> {
    TOOLS.iter().map(|t| t.name.to_string()).collect()
}

fn obj(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

/// The JSON schema of a tool's input.
pub fn schema(name: &str) -> Value {
    let id = |d: &str| json!({ "type": "string", "description": d });
    let ids = |d: &str| json!({ "type": "array", "items": {"type": "string"}, "minItems": 1, "description": d });
    let reason = json!({ "type": "string", "description": REASON });
    match name {
        "search_notes" => obj(
            json!({
                "query": {"type": "string", "description": "Words to search for. Leave out to list notes."},
                "category_id": {"type": "string", "description": "Only notes in this category."},
                "after": {"type": "string", "description": "Only notes dated on or after this day (YYYY-MM-DD) or moment (RFC 3339)."},
                "before": {"type": "string", "description": "Only notes dated before this day (YYYY-MM-DD, exclusive) or moment (RFC 3339)."},
                "date_field": {"type": "string", "enum": ["any", "created", "edited", "met", "deleted"], "description": "Which date after/before apply to: when the meeting started or the note was made or last edited (any, the default), made, edited (including saved versions), the meeting started, or moved to Trash."},
                "kind": {"type": "string", "enum": ["meeting", "note"], "description": "Only meetings or only written notes."},
                "person": {"type": "string", "description": "Only meetings this person spoke in, by name (see list_people)."},
                "tag": {"type": "string", "description": "Only notes with this tag, without the #."},
                "meeting_app": {"type": "string", "description": "Only meetings recorded in this app, e.g. Zoom."},
                "in_trash": {"type": "boolean", "description": "Search notes in Trash instead."},
                "limit": {"type": "integer", "minimum": 1, "maximum": 50, "description": "Default 20."}
            }),
            &[],
        ),
        "get_note" | "get_transcript" | "get_note_history" => obj(json!({ "note_id": id(NOTE_ID) }), &["note_id"]),
        "list_categories" | "list_people" | "list_tasks" => obj(json!({}), &[]),
        "create_note" => obj(
            json!({
                "title": {"type": "string"},
                "body_md": {"type": "string", "description": "Markdown body."},
                "category_id": {"type": "string", "description": "Category to put it in; leave out for none."},
                "reason": reason
            }),
            &["title"],
        ),
        "create_category" => obj(json!({ "name": {"type": "string"} }), &["name"]),
        "move_note" => obj(
            json!({
                "note_ids": ids("Notes to move."),
                "category_id": {"type": ["string", "null"], "description": "Target category id, or null for no category."},
                "reason": reason
            }),
            &["note_ids", "category_id"],
        ),
        "rename_category" => obj(json!({ "category_id": id("Category id."), "name": {"type": "string"} }), &["category_id", "name"]),
        "label_speaker" => obj(
            json!({
                "note_id": id(NOTE_ID),
                "speaker_id": id("Speaker id from get_transcript."),
                "label": {"type": "string", "description": "Name to show for this speaker."},
                "person_id": {"type": "string", "description": "Saved person to link this speaker to."}
            }),
            &["note_id", "speaker_id"],
        ),
        "create_task" => obj(
            json!({
                "name": {"type": "string"},
                "prompt": {"type": "string", "description": "What the task should do each run."},
                "schedule": {
                    "type": "object",
                    "description": "Local time. type every_n_days needs days; weekly needs weekday (0 = Monday … 6 = Sunday).",
                    "properties": {
                        "type": {"type": "string", "enum": ["every_n_days", "weekly", "daily"]},
                        "days": {"type": "integer", "minimum": 1, "maximum": 365},
                        "weekday": {"type": "integer", "minimum": 0, "maximum": 6},
                        "hour": {"type": "integer", "minimum": 0, "maximum": 23},
                        "minute": {"type": "integer", "minimum": 0, "maximum": 59}
                    },
                    "required": ["type", "hour", "minute"]
                },
                "scope": {
                    "type": "object",
                    "description": "Which notes it looks at. Default all_notes.",
                    "properties": {
                        "type": {"type": "string", "enum": ["all_notes", "category", "last_days"]},
                        "category_id": {"type": "string"},
                        "days": {"type": "integer", "minimum": 1}
                    },
                    "required": ["type"]
                }
            }),
            &["name", "prompt", "schedule"],
        ),
        "edit_note" => obj(
            json!({
                "note_id": id(NOTE_ID),
                "edits": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "find": {"type": "string", "description": "Exact text from the current note, occurring once. Empty to append at the end."},
                            "replace": {"type": "string", "description": "Text that replaces it."}
                        },
                        "required": ["find", "replace"],
                        "additionalProperties": false
                    }
                },
                "reason": reason
            }),
            &["note_id", "edits"],
        ),
        "rename_note" => obj(json!({ "note_id": id(NOTE_ID), "title": {"type": "string"}, "reason": reason }), &["note_id", "title"]),
        "trash_note" => obj(json!({ "note_ids": ids("Notes to move to Trash."), "reason": reason }), &["note_ids"]),
        "merge_notes" => obj(
            json!({
                "note_ids": {"type": "array", "items": {"type": "string"}, "minItems": 2, "maxItems": 10},
                "title": {"type": "string", "description": "Title of the merged note."},
                "body_md": {"type": "string", "description": "Full Markdown of the merged note, keeping every fact from the originals."},
                "reason": reason
            }),
            &["note_ids", "title", "body_md"],
        ),
        "delete_category" => obj(json!({ "category_id": id("Category id."), "reason": reason }), &["category_id"]),
        "delete_person" => obj(json!({ "person_id": id("Saved person id from list_people."), "reason": reason }), &["person_id"]),
        _ => obj(json!({}), &[]),
    }
}

/// Tool specs for a model request. `tasks_only` limits them to tools that can run in a task.
pub fn specs(tasks_only: bool) -> Vec<ToolSpec> {
    TOOLS
        .iter()
        .filter(|t| !tasks_only || t.in_tasks)
        .map(|t| ToolSpec { name: t.name.to_string(), description: t.description.to_string(), input_schema: schema(t.name) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_the_spec_table() {
        let by_tier = |tier| TOOLS.iter().filter(|t| t.tier == tier).map(|t| t.name).collect::<Vec<_>>();
        assert_eq!(
            by_tier(ToolTier::Read),
            ["search_notes", "get_note", "get_transcript", "list_categories", "list_people", "list_tasks", "get_note_history"]
        );
        assert_eq!(by_tier(ToolTier::Additive), ["create_note", "create_category", "move_note", "rename_category", "label_speaker", "create_task"]);
        assert_eq!(by_tier(ToolTier::Edit), ["edit_note", "rename_note"]);
        assert_eq!(by_tier(ToolTier::Destructive), ["trash_note", "merge_notes", "delete_category", "delete_person"]);
    }

    #[test]
    fn every_tool_has_an_object_schema() {
        for t in TOOLS {
            let s = schema(t.name);
            assert_eq!(s["type"], "object", "{}", t.name);
            assert_eq!(s["additionalProperties"], false, "{}", t.name);
        }
        // No network or shell tools, ever.
        assert!(TOOLS.iter().all(|t| !["bash", "shell", "fetch", "web_search", "http"].iter().any(|bad| t.name.contains(bad))));
    }
}
