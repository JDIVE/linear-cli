use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;
use tabled::{Table, Tabled};

use crate::api::{resolve_issue_id, LinearClient};
use crate::display_options;
use crate::output::{ensure_non_empty, filter_values, print_json, sort_values, OutputOptions};
use crate::text::truncate;

#[derive(Subcommand)]
pub enum RelationCommands {
    /// List relations for an issue (including inverse relations and children)
    #[command(alias = "ls")]
    List {
        /// Issue ID or identifier
        issue: String,
    },
    /// Add a relation between two issues
    #[command(after_help = r#"EXAMPLES:
    linear relations add ENG-1 blocks ENG-2
    linear rel add ENG-1 blocked-by ENG-2
    linear rel add ENG-1 relates-to ENG-2"#)]
    Add {
        /// Source issue ID or identifier
        issue: String,
        /// Relation type (blocks, blocked-by, duplicates, duplicate-of, relates-to)
        relation: String,
        /// Target issue ID or identifier
        target: String,
    },
    /// Remove a relation between two issues
    #[command(after_help = r#"EXAMPLES:
    linear relations remove ENG-1 blocks ENG-2
    linear rel remove ENG-1 blocked-by ENG-2"#)]
    Remove {
        /// Source issue ID or identifier
        issue: String,
        /// Relation type (blocks, blocked-by, duplicates, duplicate-of, relates-to)
        relation: String,
        /// Target issue ID or identifier
        target: String,
    },
    /// List child issues for a parent
    #[command(after_help = r#"EXAMPLE:
    linear relations children ENG-1"#)]
    Children {
        /// Parent issue ID or identifier
        issue: String,
    },
}

#[derive(Tabled)]
struct RelationRow {
    #[tabled(rename = "Type")]
    relation_type: String,
    #[tabled(rename = "Issue")]
    identifier: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "State")]
    state: String,
    #[tabled(rename = "ID")]
    id: String,
}

#[derive(Tabled)]
struct ChildRow {
    #[tabled(rename = "Issue")]
    identifier: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "State")]
    state: String,
    #[tabled(rename = "ID")]
    id: String,
}

#[derive(Debug, Clone, Copy)]
enum RelationKind {
    Blocks,
    BlockedBy,
    Duplicate,
    DuplicateOf,
    Related,
}

pub async fn handle(cmd: RelationCommands, output: &OutputOptions) -> Result<()> {
    match cmd {
        RelationCommands::List { issue } => list_relations(&issue, output).await,
        RelationCommands::Add {
            issue,
            relation,
            target,
        } => add_relation(&issue, &relation, &target, output).await,
        RelationCommands::Remove {
            issue,
            relation,
            target,
        } => remove_relation(&issue, &relation, &target, output).await,
        RelationCommands::Children { issue } => list_children(&issue, output).await,
    }
}

fn parse_relation_kind(value: &str) -> Result<RelationKind> {
    match value.to_lowercase().as_str() {
        "blocks" => Ok(RelationKind::Blocks),
        "blocked-by" | "blocked_by" => Ok(RelationKind::BlockedBy),
        "duplicates" => Ok(RelationKind::Duplicate),
        "duplicate-of" | "duplicate_of" => Ok(RelationKind::DuplicateOf),
        "relates-to" | "related" | "relates_to" => Ok(RelationKind::Related),
        _ => anyhow::bail!(
            "Invalid relation '{}'. Use blocks, blocked-by, duplicates, duplicate-of, or relates-to.",
            value
        ),
    }
}

fn format_relation_type(kind: &str, inverse: bool) -> String {
    match kind {
        "blocks" => {
            if inverse {
                "blocked-by".to_string()
            } else {
                "blocks".to_string()
            }
        }
        "duplicate" => {
            if inverse {
                "duplicate-of".to_string()
            } else {
                "duplicates".to_string()
            }
        }
        "related" => "relates-to".to_string(),
        other => other.to_string(),
    }
}

async fn list_relations(issue: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let issue_id = resolve_issue_id(&client, issue, true).await?;

    let query = r#"
        query($id: String!) {
            issue(id: $id) {
                id
                identifier
                title
                relations(first: 50) {
                    nodes {
                        id
                        type
                        relatedIssue {
                            id
                            identifier
                            title
                            state { name }
                        }
                    }
                }
                inverseRelations(first: 50) {
                    nodes {
                        id
                        type
                        issue {
                            id
                            identifier
                            title
                            state { name }
                        }
                    }
                }
                children(first: 50) {
                    nodes {
                        id
                        identifier
                        title
                        state { name }
                    }
                }
            }
        }
    "#;

    let result = client
        .query(query, Some(json!({ "id": issue_id })))
        .await?;
    let issue_data = &result["data"]["issue"];

    if issue_data.is_null() {
        anyhow::bail!("Issue not found: {}", issue);
    }

    let empty = vec![];
    let relations = issue_data["relations"]["nodes"]
        .as_array()
        .unwrap_or(&empty);
    let inverse = issue_data["inverseRelations"]["nodes"]
        .as_array()
        .unwrap_or(&empty);
    let children = issue_data["children"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    let mut relation_rows: Vec<serde_json::Value> = Vec::new();
    for rel in relations {
        relation_rows.push(json!({
            "id": rel["id"],
            "type": format_relation_type(rel["type"].as_str().unwrap_or(""), false),
            "issue": rel["relatedIssue"],
        }));
    }
    for rel in inverse {
        relation_rows.push(json!({
            "id": rel["id"],
            "type": format_relation_type(rel["type"].as_str().unwrap_or(""), true),
            "issue": rel["issue"],
        }));
    }

    if output.is_json() || output.has_template() {
        print_json(
            &json!({
                "issue": {
                    "id": issue_data["id"],
                    "identifier": issue_data["identifier"],
                    "title": issue_data["title"],
                },
                "relations": relation_rows,
                "children": children,
            }),
            output,
        )?;
        return Ok(());
    }

    let mut relation_rows = relation_rows;
    filter_values(&mut relation_rows, &output.filters);
    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut relation_rows, sort_key, output.json.order);
    }

    ensure_non_empty(&relation_rows, output)?;
    let width = display_options().max_width(50);
    let rows: Vec<RelationRow> = relation_rows
        .iter()
        .map(|r| RelationRow {
            relation_type: r["type"].as_str().unwrap_or("-").to_string(),
            identifier: r["issue"]["identifier"].as_str().unwrap_or("-").to_string(),
            title: truncate(r["issue"]["title"].as_str().unwrap_or(""), width),
            state: r["issue"]["state"]["name"].as_str().unwrap_or("-").to_string(),
            id: r["id"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    println!(
        "{} {}",
        issue_data["identifier"].as_str().unwrap_or("").bold(),
        issue_data["title"].as_str().unwrap_or("")
    );
    println!("{}", "-".repeat(50));

    if rows.is_empty() {
        println!("No relations found.");
    } else {
        let table = Table::new(rows).to_string();
        println!("{}", table);
        println!("\n{} relations", relation_rows.len());
    }

    if !children.is_empty() {
        let child_rows: Vec<ChildRow> = children
            .iter()
            .map(|c| ChildRow {
                identifier: c["identifier"].as_str().unwrap_or("-").to_string(),
                title: truncate(c["title"].as_str().unwrap_or(""), width),
                state: c["state"]["name"].as_str().unwrap_or("-").to_string(),
                id: c["id"].as_str().unwrap_or("").to_string(),
            })
            .collect();

        println!("\n{}", "Children".bold());
        println!("{}", "-".repeat(50));
        let table = Table::new(child_rows).to_string();
        println!("{}", table);
    }

    Ok(())
}

async fn list_children(issue: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let issue_id = resolve_issue_id(&client, issue, true).await?;

    let query = r#"
        query($id: String!) {
            issue(id: $id) {
                id
                identifier
                title
                children(first: 50) {
                    nodes {
                        id
                        identifier
                        title
                        state { name }
                    }
                }
            }
        }
    "#;

    let result = client
        .query(query, Some(json!({ "id": issue_id })))
        .await?;
    let issue_data = &result["data"]["issue"];

    if issue_data.is_null() {
        anyhow::bail!("Issue not found: {}", issue);
    }

    let empty = vec![];
    let children = issue_data["children"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    if output.is_json() || output.has_template() {
        print_json(
            &json!({
                "issue": {
                    "id": issue_data["id"],
                    "identifier": issue_data["identifier"],
                    "title": issue_data["title"],
                },
                "children": children,
            }),
            output,
        )?;
        return Ok(());
    }

    if children.is_empty() {
        println!("No child issues found.");
        return Ok(());
    }

    let width = display_options().max_width(50);
    let rows: Vec<ChildRow> = children
        .iter()
        .map(|c| ChildRow {
            identifier: c["identifier"].as_str().unwrap_or("-").to_string(),
            title: truncate(c["title"].as_str().unwrap_or(""), width),
            state: c["state"]["name"].as_str().unwrap_or("-").to_string(),
            id: c["id"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    println!(
        "{} {}",
        issue_data["identifier"].as_str().unwrap_or("").bold(),
        issue_data["title"].as_str().unwrap_or("")
    );
    println!("{}", "-".repeat(50));
    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} children", children.len());

    Ok(())
}

async fn add_relation(
    issue: &str,
    relation: &str,
    target: &str,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;
    let issue_id = resolve_issue_id(&client, issue, true).await?;
    let target_id = resolve_issue_id(&client, target, true).await?;
    let kind = parse_relation_kind(relation)?;

    let (issue_id, related_issue_id, rel_type) = match kind {
        RelationKind::Blocks => (issue_id, target_id, "blocks"),
        RelationKind::BlockedBy => (target_id, issue_id, "blocks"),
        RelationKind::Duplicate => (issue_id, target_id, "duplicate"),
        RelationKind::DuplicateOf => (target_id, issue_id, "duplicate"),
        RelationKind::Related => (issue_id, target_id, "related"),
    };

    let mutation = r#"
        mutation($input: IssueRelationCreateInput!) {
            issueRelationCreate(input: $input) {
                success
                issueRelation { id type }
            }
        }
    "#;

    let input = json!({
        "issueId": issue_id,
        "relatedIssueId": related_issue_id,
        "type": rel_type
    });

    let result = client
        .mutate(mutation, Some(json!({ "input": input })))
        .await?;

    if result["data"]["issueRelationCreate"]["success"].as_bool() == Some(true) {
        let relation = &result["data"]["issueRelationCreate"]["issueRelation"];

        if output.is_json() || output.has_template() {
            print_json(relation, output)?;
            return Ok(());
        }

        println!("{} Relation created", "+".green());
        println!("  ID: {}", relation["id"].as_str().unwrap_or(""));
    } else {
        anyhow::bail!("Failed to create relation");
    }

    Ok(())
}

async fn remove_relation(
    issue: &str,
    relation: &str,
    target: &str,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;
    let issue_id = resolve_issue_id(&client, issue, true).await?;
    let target_id = resolve_issue_id(&client, target, true).await?;
    let kind = parse_relation_kind(relation)?;

    let query = r#"
        query($id: String!) {
            issue(id: $id) {
                relations(first: 50) {
                    nodes {
                        id
                        type
                        relatedIssue { id }
                    }
                }
                inverseRelations(first: 50) {
                    nodes {
                        id
                        type
                        issue { id }
                    }
                }
            }
        }
    "#;

    let result = client
        .query(query, Some(json!({ "id": issue_id })))
        .await?;
    let issue_data = &result["data"]["issue"];

    if issue_data.is_null() {
        anyhow::bail!("Issue not found: {}", issue);
    }

    let empty = vec![];
    let relations = issue_data["relations"]["nodes"]
        .as_array()
        .unwrap_or(&empty);
    let inverse = issue_data["inverseRelations"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    let mut relation_id: Option<String> = None;
    match kind {
        RelationKind::Blocks | RelationKind::Duplicate | RelationKind::Related => {
            let rel_type = match kind {
                RelationKind::Blocks => "blocks",
                RelationKind::Duplicate => "duplicate",
                RelationKind::Related => "related",
                _ => "",
            };

            for rel in relations {
                let typ = rel["type"].as_str().unwrap_or("");
                let related = rel["relatedIssue"]["id"].as_str().unwrap_or("");
                if typ == rel_type && related == target_id {
                    relation_id = rel["id"].as_str().map(|s| s.to_string());
                    break;
                }
            }

            if relation_id.is_none() && matches!(kind, RelationKind::Related) {
                for rel in inverse {
                    let typ = rel["type"].as_str().unwrap_or("");
                    let related = rel["issue"]["id"].as_str().unwrap_or("");
                    if typ == "related" && related == target_id {
                        relation_id = rel["id"].as_str().map(|s| s.to_string());
                        break;
                    }
                }
            }
        }
        RelationKind::BlockedBy | RelationKind::DuplicateOf => {
            let rel_type = match kind {
                RelationKind::BlockedBy => "blocks",
                RelationKind::DuplicateOf => "duplicate",
                _ => "",
            };

            for rel in inverse {
                let typ = rel["type"].as_str().unwrap_or("");
                let related = rel["issue"]["id"].as_str().unwrap_or("");
                if typ == rel_type && related == target_id {
                    relation_id = rel["id"].as_str().map(|s| s.to_string());
                    break;
                }
            }
        }
    }

    let relation_id = relation_id.ok_or_else(|| {
        anyhow::anyhow!("No matching relation found between {} and {}", issue, target)
    })?;

    let mutation = r#"
        mutation($id: String!) {
            issueRelationDelete(id: $id) {
                success
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": relation_id })))
        .await?;

    if result["data"]["issueRelationDelete"]["success"].as_bool() == Some(true) {
        if output.is_json() || output.has_template() {
            print_json(&json!({ "deleted": true }), output)?;
            return Ok(());
        }

        println!("{} Relation removed", "+".green());
    } else {
        anyhow::bail!("Failed to remove relation");
    }

    Ok(())
}
