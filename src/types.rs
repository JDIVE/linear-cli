#![allow(dead_code)]

//! Typed structs for Linear API responses.
//!
//! These types provide structured access to Linear API data instead of working
//! with raw `serde_json::Value`. They are designed for gradual adoption - the
//! codebase can continue using `Value` for complex nested data while leveraging
//! these types for common operations.

use serde::Deserialize;

/// A Linear issue with all commonly used fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub estimate: Option<f64>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub branch_name: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub canceled_at: Option<String>,
    #[serde(default)]
    pub archived_at: Option<String>,
    #[serde(default)]
    pub state: Option<WorkflowState>,
    #[serde(default)]
    pub assignee: Option<User>,
    #[serde(default)]
    pub creator: Option<User>,
    #[serde(default)]
    pub team: Option<Team>,
    #[serde(default)]
    pub project: Option<Project>,
    #[serde(default)]
    pub cycle: Option<Cycle>,
    #[serde(default)]
    pub labels: Option<LabelConnection>,
    #[serde(default)]
    pub parent: Option<Box<IssueRef>>,
    #[serde(default)]
    pub sub_issues: Option<IssueConnection>,
}

/// A minimal issue reference for parent/child relationships.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueRef {
    pub id: String,
    pub identifier: String,
    #[serde(default)]
    pub title: Option<String>,
}

/// Connection wrapper for issues (for sub-issues, etc.).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueConnection {
    pub nodes: Vec<IssueRef>,
}

/// A workflow state (issue status).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowState {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    #[serde(default)]
    pub state_type: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub position: Option<f64>,
}

/// A Linear user.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub admin: Option<bool>,
}

/// A Linear team.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub private: Option<bool>,
}

/// A Linear project.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub slug_id: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub progress: Option<f64>,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub canceled_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// A sprint cycle.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cycle {
    pub id: String,
    #[serde(default)]
    pub number: Option<i32>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub starts_at: Option<String>,
    #[serde(default)]
    pub ends_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub progress: Option<f64>,
}

/// An issue label.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub parent: Option<Box<LabelRef>>,
}

/// A minimal label reference.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelRef {
    pub id: String,
    pub name: String,
}

/// Connection wrapper for labels.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelConnection {
    pub nodes: Vec<Label>,
}

/// A comment on an issue.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub edited_at: Option<String>,
}

/// A document in Linear.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub slug_id: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub creator: Option<User>,
    #[serde(default)]
    pub project: Option<Project>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// A notification from Linear.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    #[serde(rename = "type")]
    #[serde(default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub read_at: Option<String>,
    #[serde(default)]
    pub snoozed_until_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub issue: Option<IssueRef>,
    #[serde(default)]
    pub comment: Option<Comment>,
    #[serde(default)]
    pub actor: Option<User>,
}

/// Pagination info from GraphQL connections.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    #[serde(default)]
    pub has_next_page: bool,
    #[serde(default)]
    pub has_previous_page: bool,
    #[serde(default)]
    pub start_cursor: Option<String>,
    #[serde(default)]
    pub end_cursor: Option<String>,
}

/// A roadmap in Linear.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Roadmap {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub slug_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// An initiative in Linear.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Initiative {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub slug_id: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// A favorite item in Linear.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Favorite {
    pub id: String,
    #[serde(rename = "type")]
    #[serde(default)]
    pub favorite_type: Option<String>,
    #[serde(default)]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub issue: Option<IssueRef>,
    #[serde(default)]
    pub project: Option<Project>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// An issue relation (blocking, related, etc.).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueRelation {
    pub id: String,
    #[serde(rename = "type")]
    #[serde(default)]
    pub relation_type: Option<String>,
    #[serde(default)]
    pub issue: Option<IssueRef>,
    #[serde(default)]
    pub related_issue: Option<IssueRef>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// A time tracking entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntry {
    pub id: String,
    #[serde(default)]
    pub hours: Option<f64>,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub issue: Option<IssueRef>,
    #[serde(default)]
    pub spent_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// The current viewer (authenticated user).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewer {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub admin: Option<bool>,
}

/// Organization info.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub url_key: Option<String>,
    #[serde(default)]
    pub logo_url: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_issue_deserialize() {
        let json = r#"{
            "id": "abc123",
            "identifier": "LIN-123",
            "title": "Test issue",
            "priority": 2,
            "state": {
                "id": "state1",
                "name": "In Progress",
                "type": "started"
            },
            "assignee": {
                "id": "user1",
                "name": "John Doe",
                "email": "john@example.com"
            },
            "team": {
                "id": "team1",
                "key": "ENG",
                "name": "Engineering"
            },
            "labels": {
                "nodes": [
                    {"id": "label1", "name": "bug"},
                    {"id": "label2", "name": "urgent"}
                ]
            }
        }"#;

        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.identifier, "LIN-123");
        assert_eq!(issue.title, "Test issue");
        assert_eq!(issue.priority, 2);
        assert_eq!(issue.state.as_ref().unwrap().name, "In Progress");
        assert_eq!(issue.assignee.as_ref().unwrap().name, "John Doe");
        assert_eq!(issue.team.as_ref().unwrap().key, "ENG");
        assert_eq!(issue.labels.as_ref().unwrap().nodes.len(), 2);
    }

    #[test]
    fn test_issue_minimal() {
        let json = r#"{
            "id": "abc123",
            "identifier": "LIN-123",
            "title": "Test issue"
        }"#;

        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.identifier, "LIN-123");
        assert!(issue.state.is_none());
        assert!(issue.assignee.is_none());
    }

    #[test]
    fn test_user_deserialize() {
        let json = r#"{
            "id": "user1",
            "name": "Jane Doe",
            "email": "jane@example.com",
            "active": true
        }"#;

        let user: User = serde_json::from_str(json).unwrap();
        assert_eq!(user.name, "Jane Doe");
        assert_eq!(user.email.as_deref(), Some("jane@example.com"));
        assert_eq!(user.active, Some(true));
    }

    #[test]
    fn test_team_deserialize() {
        let json = r#"{
            "id": "team1",
            "key": "ENG",
            "name": "Engineering",
            "private": false
        }"#;

        let team: Team = serde_json::from_str(json).unwrap();
        assert_eq!(team.key, "ENG");
        assert_eq!(team.name, "Engineering");
        assert_eq!(team.private, Some(false));
    }

    #[test]
    fn test_page_info_deserialize() {
        let json = r#"{
            "hasNextPage": true,
            "hasPreviousPage": false,
            "endCursor": "abc123"
        }"#;

        let page_info: PageInfo = serde_json::from_str(json).unwrap();
        assert!(page_info.has_next_page);
        assert!(!page_info.has_previous_page);
        assert_eq!(page_info.end_cursor.as_deref(), Some("abc123"));
    }
}
