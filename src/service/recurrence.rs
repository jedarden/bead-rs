//! Recurrence template and materialization service
//!
//! This module implements R024's explicit recurring-bead materialization:
//! - Immutable recurrence template management
//! - Explicit materialization of next occurrence
//! - Series relationship tracking
//! - Idempotent materialization receipts

use crate::error::{Error, Result};
use crate::model::recurrence::{
    CreateTemplateRequest, RecurrenceMaterialization, RecurrenceTemplate,
};
use rusqlite::Connection;

/// Create a new recurrence template
pub fn create_template(
    conn: &mut Connection,
    request: CreateTemplateRequest,
) -> Result<RecurrenceTemplate> {
    let template = request.into_template(None)?;
    template.validate()?;

    // Check if template ID already exists
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM recurrence_templates WHERE id = ?)",
            [&template.id],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if exists {
        return Err(Error::conflict(format!(
            "Recurrence template already exists: {}",
            template.id
        )));
    }

    let tx = conn.unchecked_transaction()?;

    // Insert template
    let description_str = template.description.as_deref().unwrap_or("");
    let base_description_str = template.base_description.as_deref().unwrap_or("");
    let labels_json_str = template.labels_json.as_deref().unwrap_or("");

    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO recurrence_templates (id, title, description, base_title_template, base_description, priority, issue_type, labels_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        )?;

        stmt.execute((
            &template.id,
            &template.title,
            description_str,
            &template.base_title_template,
            base_description_str,
            &template.priority,
            &template.issue_type,
            labels_json_str,
            &template.created_at,
        ))?;
    }

    tx.commit()?;

    Ok(template)
}

/// Get a recurrence template by ID
pub fn get_template(conn: &Connection, template_id: &str) -> Result<RecurrenceTemplate> {
    crate::model::validate_issue_id(template_id)?;

    let result = conn.query_row(
        "SELECT id, title, description, base_title_template, base_description, priority, issue_type, labels_json, created_at
         FROM recurrence_templates WHERE id = ?",
        [template_id],
        |row| {
            Ok(RecurrenceTemplate {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                base_title_template: row.get(3)?,
                base_description: row.get(4)?,
                priority: row.get(5)?,
                issue_type: row.get(6)?,
                labels_json: row.get(7)?,
                created_at: row.get(8)?,
            })
        },
    );

    match result {
        Ok(template) => Ok(template),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(Error::not_found(format!(
            "Recurrence template not found: {}",
            template_id
        ))),
        Err(e) => Err(Error::from(e)),
    }
}

/// List all recurrence templates
pub fn list_templates(conn: &Connection) -> Result<Vec<RecurrenceTemplate>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, description, base_title_template, base_description, priority, issue_type, labels_json, created_at
         FROM recurrence_templates ORDER BY created_at DESC",
    )?;

    let templates = stmt
        .query_map([], |row| {
            Ok(RecurrenceTemplate {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                base_title_template: row.get(3)?,
                base_description: row.get(4)?,
                priority: row.get(5)?,
                issue_type: row.get(6)?,
                labels_json: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to collect templates: {}", e)))?;

    Ok(templates)
}

/// Delete a recurrence template
pub fn delete_template(conn: &mut Connection, template_id: &str) -> Result<()> {
    crate::model::validate_issue_id(template_id)?;

    // Verify template exists
    get_template(conn, template_id)?;

    let tx = conn.unchecked_transaction()?;

    // Delete template (CASCADE will handle materializations)
    let rows_affected = tx.execute(
        "DELETE FROM recurrence_templates WHERE id = ?",
        [template_id],
    )?;

    if rows_affected == 0 {
        return Err(Error::not_found(format!(
            "Recurrence template not found: {}",
            template_id
        )));
    }

    tx.commit()?;

    Ok(())
}

/// Get the next sequence number for a template
pub fn get_next_sequence(conn: &Connection, template_id: &str) -> Result<i64> {
    crate::model::validate_issue_id(template_id)?;

    // Verify template exists
    get_template(conn, template_id)?;

    let sequence: i64 = conn.query_row(
        "SELECT COALESCE(MAX(series_sequence), 0) FROM recurrence_materializations WHERE template_id = ?",
        [template_id],
        |row| row.get(0),
    )?;

    Ok(sequence + 1)
}

/// Materialize the next occurrence from a template
pub fn materialize_next_occurrence(
    conn: &mut Connection,
    template_id: &str,
    actor: Option<&str>,
) -> Result<(String, RecurrenceMaterialization)> {
    crate::model::validate_issue_id(template_id)?;

    // Get template
    let template = get_template(conn, template_id)?;

    // Get next sequence
    let sequence = get_next_sequence(conn, template_id)?;

    let tx = conn.unchecked_transaction()?;

    // Generate occurrence issue ID
    let issue_id = crate::model::generate_issue_id()?;

    // Generate title and description for occurrence
    let title = template.generate_occurrence_title(sequence);
    let description = template.base_description.as_deref();
    let labels = template.get_labels()?;

    // Create the issue using existing issue service
    let _issue = crate::service::issues::create_issue_internal(
        &tx,
        &issue_id,
        &title,
        description,
        template.priority,
        &template.issue_type,
        &labels,
        actor,
    )?;

    // Create materialization receipt
    let materialization = RecurrenceMaterialization::new(
        template_id.to_string(),
        sequence,
        issue_id.clone(),
        actor.map(|a| a.to_string()),
    );

    materialization.validate()?;

    // Store materialization receipt
    let actor_str = materialization.actor.as_deref().unwrap_or("");

    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO recurrence_materializations (template_id, series_sequence, occurrence_id, materialized_at, actor)
             VALUES (?1, ?2, ?3, ?4, ?5)"
        )?;

        stmt.execute((
            &materialization.template_id,
            &materialization.series_sequence,
            &materialization.occurrence_id,
            &materialization.materialized_at,
            actor_str,
        ))?;
    }

    tx.commit()?;

    Ok((issue_id, materialization))
}

/// Get materialization history for a template
pub fn get_materialization_history(
    conn: &Connection,
    template_id: &str,
) -> Result<Vec<RecurrenceMaterialization>> {
    crate::model::validate_issue_id(template_id)?;

    // Verify template exists
    get_template(conn, template_id)?;

    let mut stmt = conn.prepare(
        "SELECT template_id, series_sequence, occurrence_id, materialized_at, actor
         FROM recurrence_materializations WHERE template_id = ? ORDER BY series_sequence ASC",
    )?;

    let materializations = stmt
        .query_map([template_id], |row| {
            Ok(RecurrenceMaterialization {
                template_id: row.get(0)?,
                series_sequence: row.get(1)?,
                occurrence_id: row.get(2)?,
                materialized_at: row.get(3)?,
                actor: row.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| {
            Error::Internal(anyhow::anyhow!("Failed to collect materializations: {}", e))
        })?;

    Ok(materializations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::TempDir;

    fn test_store() -> (TempDir, Connection) {
        let temp_dir = TempDir::new().unwrap();
        let beads_path = temp_dir.path().join(".beads");
        std::fs::create_dir(&beads_path).unwrap();

        let db_path = beads_path.join("beads.db");
        let conn = Connection::open(&db_path).unwrap();

        // Apply migrations
        crate::store::migrations::apply_migrations(&conn).unwrap();

        (temp_dir, conn)
    }

    #[test]
    fn test_create_template() {
        let (_temp, mut conn) = test_store();

        let request = CreateTemplateRequest {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: Some("Daily standup review".to_string()),
            base_title_template: "Daily Review {n}".to_string(),
            base_description: Some("Review items for day {n}".to_string()),
            priority: Some(2),
            issue_type: Some("task".to_string()),
            labels: Some(vec!["daily".to_string(), "review".to_string()]),
        };

        let template = create_template(&mut conn, request).unwrap();
        assert_eq!(template.id, "template-001");
        assert_eq!(template.title, "Daily Review");
        assert_eq!(template.priority, 2);
    }

    #[test]
    fn test_create_duplicate_template() {
        let (_temp, mut conn) = test_store();

        let request = CreateTemplateRequest {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: None,
            base_title_template: "Daily Review {n}".to_string(),
            base_description: None,
            priority: Some(2),
            issue_type: Some("task".to_string()),
            labels: None,
        };

        create_template(&mut conn, request.clone()).unwrap();
        let result = create_template(&mut conn, request);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_template() {
        let (_temp, mut conn) = test_store();

        let request = CreateTemplateRequest {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: None,
            base_title_template: "Daily Review {n}".to_string(),
            base_description: None,
            priority: Some(2),
            issue_type: Some("task".to_string()),
            labels: None,
        };

        create_template(&mut conn, request).unwrap();
        let template = get_template(&conn, "template-001").unwrap();
        assert_eq!(template.id, "template-001");
        assert_eq!(template.title, "Daily Review");
    }

    #[test]
    fn test_get_nonexistent_template() {
        let (_temp, conn) = test_store();

        let result = get_template(&conn, "template-001");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_templates() {
        let (_temp, mut conn) = test_store();

        let request1 = CreateTemplateRequest {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: None,
            base_title_template: "Daily Review {n}".to_string(),
            base_description: None,
            priority: Some(2),
            issue_type: Some("task".to_string()),
            labels: None,
        };

        let request2 = CreateTemplateRequest {
            id: "template-002".to_string(),
            title: "Weekly Planning".to_string(),
            description: None,
            base_title_template: "Weekly Planning {n}".to_string(),
            base_description: None,
            priority: Some(3),
            issue_type: Some("task".to_string()),
            labels: None,
        };

        create_template(&mut conn, request1).unwrap();
        create_template(&mut conn, request2).unwrap();

        let templates = list_templates(&conn).unwrap();
        assert_eq!(templates.len(), 2);
    }

    #[test]
    fn test_delete_template() {
        let (_temp, mut conn) = test_store();

        let request = CreateTemplateRequest {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: None,
            base_title_template: "Daily Review {n}".to_string(),
            base_description: None,
            priority: Some(2),
            issue_type: Some("task".to_string()),
            labels: None,
        };

        create_template(&mut conn, request).unwrap();
        delete_template(&mut conn, "template-001").unwrap();

        let result = get_template(&conn, "template-001");
        assert!(result.is_err());
    }

    #[test]
    fn test_materialize_next_occurrence() {
        let (_temp, mut conn) = test_store();

        let request = CreateTemplateRequest {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: None,
            base_title_template: "Daily Review {n}".to_string(),
            base_description: None,
            priority: Some(2),
            issue_type: Some("task".to_string()),
            labels: None,
        };

        create_template(&mut conn, request).unwrap();

        let (issue_id, materialization) =
            materialize_next_occurrence(&mut conn, "template-001", Some("user")).unwrap();

        assert_eq!(materialization.series_sequence, 1);
        assert_eq!(materialization.template_id, "template-001");
        assert_eq!(materialization.occurrence_id, issue_id);
        assert_eq!(materialization.actor, Some("user".to_string()));
    }

    #[test]
    fn test_materialize_sequence_incrementing() {
        let (_temp, mut conn) = test_store();

        let request = CreateTemplateRequest {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: None,
            base_title_template: "Daily Review {n}".to_string(),
            base_description: None,
            priority: Some(2),
            issue_type: Some("task".to_string()),
            labels: None,
        };

        create_template(&mut conn, request).unwrap();

        let (_issue_id1, mat1) =
            materialize_next_occurrence(&mut conn, "template-001", Some("user")).unwrap();
        assert_eq!(mat1.series_sequence, 1);

        let (_issue_id2, mat2) =
            materialize_next_occurrence(&mut conn, "template-001", Some("user")).unwrap();
        assert_eq!(mat2.series_sequence, 2);
    }

    #[test]
    fn test_get_next_sequence() {
        let (_temp, mut conn) = test_store();

        let request = CreateTemplateRequest {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: None,
            base_title_template: "Daily Review {n}".to_string(),
            base_description: None,
            priority: Some(2),
            issue_type: Some("task".to_string()),
            labels: None,
        };

        create_template(&mut conn, request).unwrap();

        assert_eq!(get_next_sequence(&conn, "template-001").unwrap(), 1);

        materialize_next_occurrence(&mut conn, "template-001", Some("user")).unwrap();
        assert_eq!(get_next_sequence(&conn, "template-001").unwrap(), 2);
    }

    #[test]
    fn test_get_materialization_history() {
        let (_temp, mut conn) = test_store();

        let request = CreateTemplateRequest {
            id: "template-001".to_string(),
            title: "Daily Review".to_string(),
            description: None,
            base_title_template: "Daily Review {n}".to_string(),
            base_description: None,
            priority: Some(2),
            issue_type: Some("task".to_string()),
            labels: None,
        };

        create_template(&mut conn, request).unwrap();

        materialize_next_occurrence(&mut conn, "template-001", Some("user")).unwrap();
        materialize_next_occurrence(&mut conn, "template-001", Some("user")).unwrap();

        let history = get_materialization_history(&conn, "template-001").unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].series_sequence, 1);
        assert_eq!(history[1].series_sequence, 2);
    }
}
