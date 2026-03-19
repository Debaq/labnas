use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub requires_confirmation: bool,
    #[serde(default)]
    pub insistent: bool,
    #[serde(default)]
    pub confirmed_by: Vec<String>,
    #[serde(default)]
    pub rejected_by: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub last_reminder: Option<DateTime<Utc>>,
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
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TasksConfig {
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub tasks: Vec<Task>,
}
