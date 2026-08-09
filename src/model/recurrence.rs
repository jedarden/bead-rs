//! Recurrence template and materialization models
//!
//! This module defines the data structures for R024's explicit recurring-bead materialization:
//! - Immutable recurrence templates
//! - Materialization receipts
//! - Series relationships between templates and occurrences

use serde::{Deserialize, Serialize};

/// Immutable recurrence template that defines how recurring issues should be created
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurrenceTemplate {
    /// Unique template identifier
    pub id: String,
    /// Human-readable template title
    pub title: String,
    /// Optional template description
    pub description: Option<String>,
    /// Title template for occurrences (may include {n} for sequence number)
    pub base_title_template: String,
    /// Description template for occurrences
    pub base_description: Option<String>,
    /// Priority for created issues (0-4)
    pub priority: i64,
    /// Issue type for created issues
    pub issue_type: String,
    /// JSON array of labels to apply to occurrences
    pub labels_json: Option<String>,
    /// Template creation timestamp
    pub created_at: String,
}

impl RecurrenceTemplate {
    /// Validate a recurrence template
    pub fn validate(&self) -> Result<(), crate::error::Error> {
        // Validate template ID
        crate::model::validate_issue_id(&self.id)?;

        // Validate title
        if self.title.is_empty() || self.title.len() > 4096 {
            return Err(crate::error::Error::validation(
                "Template title must be 1-4096 bytes",
            ));
        }

        // Validate title template
        if self.base_title_template.is_empty() || self.base_title_template.len() > 4096 {
            return Err(crate::error::Error::validation(
                "Title template must be 1-4096 bytes",
            ));
        }

        // Validate priority
        crate::model::validate_priority(self.priority)?;

        // Validate issue type
        if self.issue_type.is_empty() || self.issue_type.len() > 255 {
            return Err(crate::error::Error::validation(
                "Issue type must be 1-255 bytes",
            ));
        }

        // Validate labels JSON if present
        if let Some(ref labels_json) = self.labels_json {
            if labels_json.len() > 65536 {
                return Err(crate::error::Error::validation(
                    "Labels JSON cannot exceed 65536 bytes",
                ));
            }

            // Try to parse as JSON array
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(labels_json);
            if let Ok(value) = parsed {
                if !value.is_array() {
                    return Err(crate::error::Error::validation(
                        "Labels JSON must be an array",
                    ));
                }
            } else {
                return Err(crate::error::Error::validation(
                    "Labels JSON must be valid JSON",
                ));
            }
        }

        Ok(())
    }

    /// Generate title for next occurrence
    pub fn generate_occurrence_title(&self, sequence: i64) -> String {
        self.base_title_template
            .replace("{n}", &sequence.to_string())
    }

    /// Get labels as a vector if present
    pub fn get_labels(&self) -> Result<Vec<String>, crate::error::Error> {
        match &self.labels_json {
            None => Ok(Vec::new()),
            Some(json_str) if json_str.is_empty() => Ok(Vec::new()),
            Some(json_str) => {
                let parsed: serde_json::Value = serde_json::from_str(json_str)
                    .map_err(|_| crate::error::Error::validation("Invalid labels JSON"))?;

                if let Some(arr) = parsed.as_array() {
                    let labels = arr
                        .iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>();

                    // Validate each label
                    for label in &labels {
                        crate::model::validate_label(label)?;
                    }

                    Ok(labels)
                } else {
                    Err(crate::error::Error::validation(
                        "Labels JSON must be an array",
                    ))
                }
            }
        }
    }
}

/// Materialization receipt tracking the relationship between a template and occurrence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurrenceMaterialization {
    /// Template ID that generated this occurrence
    pub template_id: String,
    /// Sequence number in the series (1, 2, 3, ...)
    pub series_sequence: i64,
    /// The created issue ID
    pub occurrence_id: String,
    /// When the occurrence was materialized
    pub materialized_at: String,
    /// Who/materialization that created the occurrence
    pub actor: Option<String>,
}

impl RecurrenceMaterialization {
    /// Create a new materialization receipt
    pub fn new(
        template_id: String,
        series_sequence: i64,
        occurrence_id: String,
        actor: Option<String>,
    ) -> Self {
        let materialized_at = crate::model::current_timestamp();

        Self {
            template_id,
            series_sequence,
            occurrence_id,
            materialized_at,
            actor,
        }
    }

    /// Validate a materialization receipt
    pub fn validate(&self) -> Result<(), crate::error::Error> {
        // Validate template ID
        crate::model::validate_issue_id(&self.template_id)?;

        // Validate occurrence ID
        crate::model::validate_issue_id(&self.occurrence_id)?;

        // Validate sequence is positive
        if self.series_sequence < 1 {
            return Err(crate::error::Error::validation(
                "Series sequence must be >= 1",
            ));
        }

        // Validate actor if present
        if let Some(ref actor) = self.actor {
            if actor.is_empty() || actor.len() > 255 {
                return Err(crate::error::Error::validation(
                    "Actor must be 1-255 bytes if present",
                ));
            }
        }

        Ok(())
    }
}

/// Template creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTemplateRequest {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub base_title_template: String,
    pub base_description: Option<String>,
    pub priority: Option<i64>,
    pub issue_type: Option<String>,
    pub labels: Option<Vec<String>>,
}

impl CreateTemplateRequest {
    /// Validate and convert to a template
    pub fn into_template(
        self,
        _creator: Option<&str>,
    ) -> Result<RecurrenceTemplate, crate::error::Error> {
        let created_at = crate::model::current_timestamp();
        let priority = self.priority.unwrap_or(2);
        let issue_type = self.issue_type.unwrap_or_else(|| "task".to_string());
        let labels_json =
            if let Some(labels) = self.labels {
                if labels.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(&labels).map_err(|_| {
                        crate::error::Error::validation("Failed to serialize labels")
                    })?)
                }
            } else {
                None
            };

        let template = RecurrenceTemplate {
            id: self.id.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            base_title_template: self.base_title_template.clone(),
            base_description: self.base_description.clone(),
            priority,
            issue_type,
            labels_json,
            created_at,
        };

        template.validate()?;
        Ok(template)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_validation() {
        let template = RecurrenceTemplate {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: Some("Daily standup review".to_string()),
            base_title_template: "Daily Review {n}".to_string(),
            base_description: Some("Review items for day {n}".to_string()),
            priority: 2,
            issue_type: "task".to_string(),
            labels_json: Some(r#"["daily","review"]"#.to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        assert!(template.validate().is_ok());
    }

    #[test]
    fn test_template_validation_invalid_priority() {
        let template = RecurrenceTemplate {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: None,
            base_title_template: "Daily Review {n}".to_string(),
            base_description: None,
            priority: 5, // Invalid priority
            issue_type: "task".to_string(),
            labels_json: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        assert!(template.validate().is_err());
    }

    #[test]
    fn test_generate_occurrence_title() {
        let template = RecurrenceTemplate {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: None,
            base_title_template: "Daily Review {n}".to_string(),
            base_description: None,
            priority: 2,
            issue_type: "task".to_string(),
            labels_json: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        assert_eq!(template.generate_occurrence_title(1), "Daily Review 1");
        assert_eq!(template.generate_occurrence_title(10), "Daily Review 10");
    }

    #[test]
    fn test_materialization_validation() {
        let materialization = RecurrenceMaterialization {
            template_id: "template-001".to_string(),
            series_sequence: 1,
            occurrence_id: "bead-001".to_string(),
            materialized_at: "2024-01-01T00:00:00Z".to_string(),
            actor: Some("user".to_string()),
        };

        assert!(materialization.validate().is_ok());
    }

    #[test]
    fn test_materialization_validation_invalid_sequence() {
        let materialization = RecurrenceMaterialization {
            template_id: "template-001".to_string(),
            series_sequence: 0, // Invalid sequence
            occurrence_id: "bead-001".to_string(),
            materialized_at: "2024-01-01T00:00:00Z".to_string(),
            actor: Some("user".to_string()),
        };

        assert!(materialization.validate().is_err());
    }

    #[test]
    fn test_get_labels() {
        let template = RecurrenceTemplate {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: None,
            base_title_template: "Daily Review {n}".to_string(),
            base_description: None,
            priority: 2,
            issue_type: "task".to_string(),
            labels_json: Some(r#"["daily","review"]"#.to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let labels = template.get_labels().unwrap();
        assert_eq!(labels, vec!["daily".to_string(), "review".to_string()]);
    }
}
