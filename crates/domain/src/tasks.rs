use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{Id, Proposal, Timestamp};

/// Which notes a scheduled task looks at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskScope {
    AllNotes,
    Category { category_id: Id },
    LastDays { days: u32 },
}

/// When a scheduled task runs. Times are local wall-clock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskSchedule {
    EveryNDays {
        days: u32,
        hour: u8,
        minute: u8,
    },
    /// `weekday`: 0 = Monday … 6 = Sunday.
    Weekly {
        weekday: u8,
        hour: u8,
        minute: u8,
    },
    Daily {
        hour: u8,
        minute: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TaskRunStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Task {
    pub id: Id,
    pub name: String,
    pub prompt: String,
    pub scope: TaskScope,
    pub schedule: TaskSchedule,
    pub next_run_at: Option<Timestamp>,
    pub enabled: bool,
    /// The built-in Notes review task. It can be edited and disabled but not deleted.
    pub is_default: bool,
    pub last_run: Option<TaskRun>,
    /// Pending proposals from its runs, for the sidebar badge.
    pub pending_count: u32,
}

/// The editable fields of a task, for create and update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct TaskDraft {
    pub name: String,
    pub prompt: String,
    pub scope: TaskScope,
    pub schedule: TaskSchedule,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct TaskRun {
    pub id: Id,
    pub task_id: Id,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
    pub status: TaskRunStatus,
    /// A short plain-text summary of what the run found.
    pub summary: String,
    pub error: Option<String>,
}

/// A run with everything it proposed: the report view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct TaskRunDetail {
    pub run: TaskRun,
    pub proposals: Vec<Proposal>,
}
