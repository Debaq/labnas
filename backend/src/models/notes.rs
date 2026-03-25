use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String, // Markdown
    pub created_by: String,
    pub updated_by: String,
    #[serde(default)]
    pub shared_with: Vec<String>,
    #[serde(default)]
    pub is_public: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
