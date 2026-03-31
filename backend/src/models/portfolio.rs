use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PortfolioType {
    Project,
    Course,
    Diploma,
    Workshop,
}

impl Default for PortfolioType {
    fn default() -> Self {
        Self::Project
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PortfolioStatus {
    Planned,
    Active,
    Completed,
    Cancelled,
    Submitted,
}

/// Alcance: propio (participamos), externo (referencia), historico (archivo)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PortfolioScope {
    Own,       // Participamos / lideramos
    External,  // Externo, solo referencia informativa
    Historic,  // Historico / archivo
}

impl Default for PortfolioScope {
    fn default() -> Self {
        Self::Own
    }
}

impl Default for PortfolioStatus {
    fn default() -> Self {
        Self::Planned
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioEntry {
    pub id: String,
    #[serde(default)]
    pub entry_type: PortfolioType,
    #[serde(default)]
    pub scope: PortfolioScope,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub institution: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub contact: String,
    #[serde(default)]
    pub funding_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<f64>,
    #[serde(default)]
    pub principal_investigator: String,
    #[serde(default)]
    pub collaborators: Vec<String>,
    #[serde(default)]
    pub participants: Vec<String>,
    #[serde(default)]
    pub start_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<u32>,
    #[serde(default)]
    pub modality: String,
    #[serde(default)]
    pub status: PortfolioStatus,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
    #[serde(default)]
    pub milestones: Vec<Milestone>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub related_files: Vec<String>,
    #[serde(default)]
    pub related_inventory: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PortfolioConfig {
    #[serde(default)]
    pub entries: Vec<PortfolioEntry>,
}
