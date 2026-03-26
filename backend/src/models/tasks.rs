use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pendiente,
    EnProgreso,
    Completada,
    Rechazada,
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::Pendiente
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub project_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub assigned_to: Vec<String>, // chat names or "all"
    pub status: TaskStatus,
    pub created_by: String,
    pub due_date: Option<String>, // "2026-03-20"
    #[serde(default)]
    pub due_time: Option<String>, // "14:30"
    #[serde(default)]
    pub requires_confirmation: bool,
    #[serde(default)]
    pub insistent: bool,
    #[serde(default = "default_reminder_minutes")]
    pub reminder_minutes: u32,
    #[serde(default)]
    pub confirmed_by: Vec<String>,
    #[serde(default)]
    pub rejected_by: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub last_reminder: Option<DateTime<Utc>>,
}

fn default_reminder_minutes() -> u32 {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub created_by: String,
    #[serde(default)]
    pub members: Vec<String>,
    /// Tags por miembro: { "nick": ["backend", "devops"], "ana": ["frontend"] }
    #[serde(default)]
    pub member_tags: HashMap<String, Vec<String>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub date: String,      // "2026-03-20"
    pub time: String,      // "14:30"
    pub created_by: String,
    #[serde(default)]
    pub invitees: Vec<String>, // usernames or "all"
    #[serde(default)]
    pub accepted: Vec<String>,
    #[serde(default)]
    pub declined: Vec<String>,
    #[serde(default = "default_event_reminder")]
    pub remind_before_min: u32, // avisar N min antes
    #[serde(default)]
    pub reminded: bool,
    /// Recurrencia: "none", "daily", "weekly", "monthly"
    #[serde(default)]
    pub recurrence: String,
    /// Fecha fin de recurrencia (opcional, "2026-12-31")
    #[serde(default)]
    pub recurrence_end: Option<String>,
    pub created_at: DateTime<Utc>,
}

fn default_event_reminder() -> u32 {
    15
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TasksConfig {
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub events: Vec<CalendarEvent>,
}
