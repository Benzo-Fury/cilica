//! Links between notes and the graph drawn from them (docs/spec.md › Links and the graph).
//!
//! Notes link with Obsidian's syntax: `[[Title]]`, `[[Title|shown text]]`, `[[Title#Heading]]`,
//! `![[Title]]` embeds, `[text](Title.md)` and `#tags`. Nothing here is stored as such: the store
//! reads it from the Markdown.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{Id, NoteKind, NoteRef};

/// What a node in the graph stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeKind {
    Note,
    /// A saved person: linked by name (`[[Sam Ortiz]]`) or heard in a meeting.
    Person,
    Tag,
    /// A link to a note that is not created yet.
    Missing,
}

/// How two nodes are joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GraphEdgeKind {
    Link,
    Embed,
    Tag,
    /// The person spoke in the meeting. Read from the transcript; never written into the note.
    Transcript,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct GraphNode {
    /// Stable across calls: `n:<note id>`, `p:<person id>`, `t:<tag>`, `m:<title>`.
    pub key: String,
    pub kind: GraphNodeKind,
    pub label: String,
    pub note_id: Option<Id>,
    pub note_kind: Option<NoteKind>,
    pub category_id: Option<Id>,
    pub person_id: Option<Id>,
    /// A person's colour (1–8).
    pub color_index: Option<u32>,
    /// How many edges touch the node in the full graph (sizes it).
    pub degree: u32,
}

/// An undirected edge between two entries of `LinkGraph::nodes`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct GraphEdge {
    pub source: u32,
    pub target: u32,
    pub kind: GraphEdgeKind,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct LinkGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// The focused note's index in `nodes` (local graphs).
    pub focus: Option<u32>,
}

/// Another note that mentions this one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Mention {
    pub note: NoteRef,
    /// The line it is mentioned on, as plain text.
    pub excerpt: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct Backlinks {
    /// Notes that link here.
    pub linked: Vec<Mention>,
    /// Notes that name this one's title without linking it.
    pub unlinked: Vec<Mention>,
}
