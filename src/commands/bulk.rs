use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use futures::future::join_all;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead};
use std::path::Path;

use crate::api::{resolve_cycle_id, resolve_project_id, resolve_team_id, LinearClient};
use crate::display_options;
use crate::output::{print_json, OutputOptions};
use crate::text::truncate;

const BATCH_SIZE: usize = 50;

#[derive(Subcommand)]
pub enum BulkCommands {
    /// Update the state of multiple issues
    #[command(alias = "state")]
    #[command(after_help = r#"EXAMPLES:
    linear bulk update-state Done -i LIN-1,LIN-2,LIN-3
    linear b state "In Progress" -i LIN-1,LIN-2"#)]
    UpdateState {
        /// The new state name or ID
        state: String,
        /// Comma-separated list of issue IDs (e.g., "LIN-1,LIN-2,LIN-3")
        #[arg(short, long, value_delimiter = ',')]
        issues: Vec<String>,
    },
    /// Assign multiple issues to a user
    #[command(after_help = r#"EXAMPLES:
    linear bulk assign me -i LIN-1,LIN-2,LIN-3
    linear b assign john@example.com -i LIN-1,LIN-2"#)]
    Assign {
        /// The user to assign (user ID, name, email, or "me")
        user: String,
        /// Comma-separated list of issue IDs (e.g., "LIN-1,LIN-2,LIN-3")
        #[arg(short, long, value_delimiter = ',')]
        issues: Vec<String>,
    },
    /// Add a label to multiple issues
    #[command(after_help = r#"EXAMPLES:
    linear bulk label "Bug" -i LIN-1,LIN-2,LIN-3
    linear b label LABEL_ID -i LIN-1,LIN-2"#)]
    Label {
        /// The label name or ID to add
        label: String,
        /// Comma-separated list of issue IDs (e.g., "LIN-1,LIN-2,LIN-3")
        #[arg(short, long, value_delimiter = ',')]
        issues: Vec<String>,
    },
    /// Unassign multiple issues
    #[command(after_help = r#"EXAMPLES:
    linear bulk unassign -i LIN-1,LIN-2,LIN-3"#)]
    Unassign {
        /// Comma-separated list of issue IDs (e.g., "LIN-1,LIN-2,LIN-3")
        #[arg(short, long, value_delimiter = ',')]
        issues: Vec<String>,
    },
    /// Update priority on multiple issues
    #[command(after_help = r#"EXAMPLES:
    linear bulk priority 2 -i LIN-1,LIN-2,LIN-3"#)]
    Priority {
        /// Priority (0=none, 1=urgent, 2=high, 3=normal, 4=low)
        priority: i32,
        /// Comma-separated list of issue IDs (e.g., "LIN-1,LIN-2,LIN-3")
        #[arg(short, long, value_delimiter = ',')]
        issues: Vec<String>,
    },
    /// Move multiple issues to a project
    #[command(after_help = r#"EXAMPLES:
    linear bulk project "Q1 Roadmap" -i LIN-1,LIN-2,LIN-3"#)]
    Project {
        /// Project name or ID
        project: String,
        /// Comma-separated list of issue IDs (e.g., "LIN-1,LIN-2,LIN-3")
        #[arg(short, long, value_delimiter = ',')]
        issues: Vec<String>,
    },
    /// Move multiple issues to a cycle
    #[command(after_help = r#"EXAMPLES:
    linear bulk cycle "Cycle 12" -i LIN-1,LIN-2,LIN-3"#)]
    Cycle {
        /// Cycle name/number or ID
        cycle: String,
        /// Comma-separated list of issue IDs (e.g., "LIN-1,LIN-2,LIN-3")
        #[arg(short, long, value_delimiter = ',')]
        issues: Vec<String>,
    },
    /// Archive multiple issues
    #[command(after_help = r#"EXAMPLES:
    linear bulk archive -i LIN-1,LIN-2,LIN-3"#)]
    Archive {
        /// Comma-separated list of issue IDs (e.g., "LIN-1,LIN-2,LIN-3")
        #[arg(short, long, value_delimiter = ',')]
        issues: Vec<String>,
    },
    /// Create multiple issues using issueBatchCreate
    #[command(after_help = r#"EXAMPLES:
    linear bulk create --data '[{"title":"Task A","teamId":"TEAM_UUID"},{"title":"Task B","teamId":"TEAM_UUID"}]'
    cat issues.json | linear bulk create --data -

Input supports either:
  - JSON array of IssueCreateInput objects
  - JSON object with an "issues" array

Convenience fields are supported per issue object and auto-resolved:
  - team -> teamId
  - assignee -> assigneeId
  - project -> projectId
  - labels -> labelIds"#)]
    Create {
        /// JSON payload, file path, @file, or "-" for stdin
        #[arg(short, long)]
        data: String,
    },
}

/// Result of a single bulk operation
#[derive(Debug, Clone)]
struct BulkResult {
    issue_id: String,
    success: bool,
    identifier: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct IssueInfo {
    issue_id: String,
    uuid: String,
    team_id: String,
    identifier: Option<String>,
}

/// Check if a string looks like a UUID (contains dashes and is 36 characters)
fn is_uuid(s: &str) -> bool {
    s.len() == 36 && s.chars().filter(|c| *c == '-').count() == 4
}

/// Resolve a user identifier to a UUID.
/// Handles "me", UUIDs, names, and emails.
async fn resolve_user_id(client: &LinearClient, user: &str) -> Result<String> {
    if user.eq_ignore_ascii_case("me") {
        let query = r#"
            query {
                viewer {
                    id
                }
            }
        "#;
        let result = client.query(query, None).await?;
        let user_id = result["data"]["viewer"]["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Could not fetch current user ID"))?;
        return Ok(user_id.to_string());
    }

    if is_uuid(user) {
        return Ok(user.to_string());
    }

    let query = r#"
        query {
            users(first: 100) {
                nodes {
                    id
                    name
                    email
                }
            }
        }
    "#;

    let result = client.query(query, None).await?;
    let empty = vec![];
    let users = result["data"]["users"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    for u in users {
        let name = u["name"].as_str().unwrap_or("");
        let email = u["email"].as_str().unwrap_or("");

        if name.eq_ignore_ascii_case(user) || email.eq_ignore_ascii_case(user) {
            if let Some(id) = u["id"].as_str() {
                return Ok(id.to_string());
            }
        }
    }

    anyhow::bail!("User not found: {}", user)
}

/// Resolve a state name to a UUID for a given team.
async fn resolve_state_id(client: &LinearClient, team_id: &str, state: &str) -> Result<String> {
    if is_uuid(state) {
        return Ok(state.to_string());
    }

    let query = r#"
        query($teamId: String!) {
            team(id: $teamId) {
                states {
                    nodes {
                        id
                        name
                    }
                }
            }
        }
    "#;

    let result = client
        .query(query, Some(json!({ "teamId": team_id })))
        .await?;
    let empty = vec![];
    let states = result["data"]["team"]["states"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    for s in states {
        let name = s["name"].as_str().unwrap_or("");
        if name.eq_ignore_ascii_case(state) {
            if let Some(id) = s["id"].as_str() {
                return Ok(id.to_string());
            }
        }
    }

    anyhow::bail!("State '{}' not found for team", state)
}

/// Resolve a label name to a UUID.
async fn resolve_label_id(client: &LinearClient, label: &str) -> Result<String> {
    if is_uuid(label) {
        return Ok(label.to_string());
    }

    let query = r#"
        query {
            issueLabels(first: 250) {
                nodes {
                    id
                    name
                }
            }
        }
    "#;

    let result = client.query(query, None).await?;
    let empty = vec![];
    let labels = result["data"]["issueLabels"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    for l in labels {
        let name = l["name"].as_str().unwrap_or("");
        if name.eq_ignore_ascii_case(label) {
            if let Some(id) = l["id"].as_str() {
                return Ok(id.to_string());
            }
        }
    }

    anyhow::bail!("Label not found: {}", label)
}

/// Get issue details including UUID and team ID from identifier (e.g., "LIN-123")
async fn get_issue_info(
    client: &LinearClient,
    issue_id: &str,
) -> Result<(String, String, Option<String>)> {
    let query = r#"
        query($id: String!) {
            issue(id: $id) {
                id
                identifier
                team {
                    id
                }
            }
        }
    "#;

    let result = client.query(query, Some(json!({ "id": issue_id }))).await?;
    let issue = &result["data"]["issue"];

    if issue.is_null() {
        anyhow::bail!("Issue not found: {}", issue_id);
    }

    let uuid = issue["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Failed to get issue ID"))?
        .to_string();

    let team_id = issue["team"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Failed to get team ID"))?
        .to_string();

    let identifier = issue["identifier"].as_str().map(|s| s.to_string());

    Ok((uuid, team_id, identifier))
}

pub async fn handle(cmd: BulkCommands, output: &OutputOptions) -> Result<()> {
    match cmd {
        BulkCommands::UpdateState { state, issues } => {
            bulk_update_state(&state, issues, output).await
        }
        BulkCommands::Assign { user, issues } => bulk_assign(&user, issues, output).await,
        BulkCommands::Label { label, issues } => bulk_label(&label, issues, output).await,
        BulkCommands::Unassign { issues } => bulk_unassign(issues, output).await,
        BulkCommands::Priority { priority, issues } => {
            bulk_update_priority(priority, issues, output).await
        }
        BulkCommands::Project { project, issues } => {
            bulk_move_project(&project, issues, output).await
        }
        BulkCommands::Cycle { cycle, issues } => bulk_move_cycle(&cycle, issues, output).await,
        BulkCommands::Archive { issues } => bulk_archive(issues, output).await,
        BulkCommands::Create { data } => bulk_create(&data, output).await,
    }
}

fn ensure_issues_present(issues: &[String], output: &OutputOptions) -> Result<bool> {
    if !issues.is_empty() {
        return Ok(true);
    }

    if output.is_json() || output.has_template() {
        print_json(
            &json!({ "error": "No issues specified", "results": [] }),
            output,
        )?;
    } else {
        println!("No issues specified.");
    }
    Ok(false)
}

async fn resolve_issue_infos(
    client: &LinearClient,
    issues: &[String],
) -> (Vec<IssueInfo>, Vec<BulkResult>) {
    let futures: Vec<_> = issues
        .iter()
        .map(|issue_id| {
            let issue_id = issue_id.clone();
            async move {
                match get_issue_info(client, &issue_id).await {
                    Ok((uuid, team_id, identifier)) => Ok(IssueInfo {
                        issue_id,
                        uuid,
                        team_id,
                        identifier,
                    }),
                    Err(e) => Err(BulkResult {
                        issue_id,
                        success: false,
                        identifier: None,
                        error: Some(e.to_string()),
                    }),
                }
            }
        })
        .collect();

    let mut infos = Vec::new();
    let mut failures = Vec::new();
    for result in join_all(futures).await {
        match result {
            Ok(info) => infos.push(info),
            Err(failure) => failures.push(failure),
        }
    }
    (infos, failures)
}

async fn batch_update_infos(
    client: &LinearClient,
    infos: &[IssueInfo],
    input: Value,
) -> Vec<BulkResult> {
    let mutation = r#"
        mutation($ids: [UUID!]!, $input: IssueUpdateInput!) {
            issueBatchUpdate(ids: $ids, input: $input) {
                success
                issues {
                    id
                    identifier
                }
            }
        }
    "#;

    let mut results = Vec::new();

    for chunk in infos.chunks(BATCH_SIZE) {
        let ids: Vec<String> = chunk.iter().map(|info| info.uuid.clone()).collect();

        match client
            .mutate(
                mutation,
                Some(json!({
                    "ids": ids,
                    "input": input,
                })),
            )
            .await
        {
            Ok(data) => {
                let success = data["data"]["issueBatchUpdate"]["success"].as_bool() == Some(true);
                if success {
                    let mut identifier_by_uuid: HashMap<String, String> = HashMap::new();
                    if let Some(updated) = data["data"]["issueBatchUpdate"]["issues"].as_array() {
                        for issue in updated {
                            if let (Some(id), Some(identifier)) =
                                (issue["id"].as_str(), issue["identifier"].as_str())
                            {
                                identifier_by_uuid.insert(id.to_string(), identifier.to_string());
                            }
                        }
                    }

                    for info in chunk {
                        let identifier = identifier_by_uuid
                            .get(&info.uuid)
                            .cloned()
                            .or_else(|| info.identifier.clone());
                        results.push(BulkResult {
                            issue_id: info.issue_id.clone(),
                            success: true,
                            identifier,
                            error: None,
                        });
                    }
                } else {
                    for info in chunk {
                        results.push(BulkResult {
                            issue_id: info.issue_id.clone(),
                            success: false,
                            identifier: info.identifier.clone(),
                            error: Some("Batch update failed".to_string()),
                        });
                    }
                }
            }
            Err(e) => {
                for info in chunk {
                    results.push(BulkResult {
                        issue_id: info.issue_id.clone(),
                        success: false,
                        identifier: info.identifier.clone(),
                        error: Some(e.to_string()),
                    });
                }
            }
        }
    }

    results
}

async fn bulk_update_state(state: &str, issues: Vec<String>, output: &OutputOptions) -> Result<()> {
    if !ensure_issues_present(&issues, output)? {
        return Ok(());
    }

    if !output.is_json() && !output.has_template() {
        println!(
            "{} Updating state to '{}' for {} issues...",
            ">>".cyan(),
            state,
            issues.len()
        );
    }

    let client = LinearClient::new()?;
    let (infos, mut failures) = resolve_issue_infos(&client, &issues).await;

    let mut grouped: HashMap<String, Vec<IssueInfo>> = HashMap::new();
    for info in infos {
        grouped.entry(info.team_id.clone()).or_default().push(info);
    }

    let mut successes = Vec::new();
    for (team_id, team_infos) in grouped {
        match resolve_state_id(&client, &team_id, state).await {
            Ok(state_id) => {
                let mut team_results =
                    batch_update_infos(&client, &team_infos, json!({ "stateId": state_id })).await;
                successes.append(&mut team_results);
            }
            Err(e) => {
                for info in team_infos {
                    failures.push(BulkResult {
                        issue_id: info.issue_id,
                        success: false,
                        identifier: info.identifier,
                        error: Some(e.to_string()),
                    });
                }
            }
        }
    }

    failures.extend(successes);
    print_summary(&failures, "state updated", output);
    Ok(())
}

async fn bulk_assign(user: &str, issues: Vec<String>, output: &OutputOptions) -> Result<()> {
    if !ensure_issues_present(&issues, output)? {
        return Ok(());
    }

    if !output.is_json() && !output.has_template() {
        println!(
            "{} Assigning {} issues to '{}'...",
            ">>".cyan(),
            issues.len(),
            user
        );
    }

    let client = LinearClient::new()?;
    let user_id = match resolve_user_id(&client, user).await {
        Ok(id) => id,
        Err(e) => {
            if output.is_json() || output.has_template() {
                print_json(
                    &json!({ "error": format!("Failed to resolve user '{}': {}", user, e), "results": [] }),
                    output,
                )?;
            } else {
                println!("{} Failed to resolve user '{}': {}", "x".red(), user, e);
            }
            return Ok(());
        }
    };

    let (infos, mut failures) = resolve_issue_infos(&client, &issues).await;
    let mut updated = batch_update_infos(&client, &infos, json!({ "assigneeId": user_id })).await;
    failures.append(&mut updated);
    print_summary(&failures, "assigned", output);
    Ok(())
}

async fn bulk_label(label: &str, issues: Vec<String>, output: &OutputOptions) -> Result<()> {
    if !ensure_issues_present(&issues, output)? {
        return Ok(());
    }

    if !output.is_json() && !output.has_template() {
        println!(
            "{} Adding label '{}' to {} issues...",
            ">>".cyan(),
            label,
            issues.len()
        );
    }

    let client = LinearClient::new()?;

    let label_id = match resolve_label_id(&client, label).await {
        Ok(id) => id,
        Err(e) => {
            if output.is_json() || output.has_template() {
                print_json(
                    &json!({ "error": format!("Failed to resolve label '{}': {}", label, e), "results": [] }),
                    output,
                )?;
            } else {
                println!("{} Failed to resolve label '{}': {}", "x".red(), label, e);
            }
            return Ok(());
        }
    };

    let (infos, mut failures) = resolve_issue_infos(&client, &issues).await;
    let mut updated =
        batch_update_infos(&client, &infos, json!({ "addedLabelIds": [label_id] })).await;
    failures.append(&mut updated);
    print_summary(&failures, "labeled", output);
    Ok(())
}

async fn bulk_unassign(issues: Vec<String>, output: &OutputOptions) -> Result<()> {
    if !ensure_issues_present(&issues, output)? {
        return Ok(());
    }

    if !output.is_json() && !output.has_template() {
        println!("{} Unassigning {} issues...", ">>".cyan(), issues.len());
    }

    let client = LinearClient::new()?;
    let (infos, mut failures) = resolve_issue_infos(&client, &issues).await;
    let mut updated = batch_update_infos(&client, &infos, json!({ "assigneeId": null })).await;
    failures.append(&mut updated);
    print_summary(&failures, "unassigned", output);
    Ok(())
}

async fn bulk_update_priority(
    priority: i32,
    issues: Vec<String>,
    output: &OutputOptions,
) -> Result<()> {
    if !ensure_issues_present(&issues, output)? {
        return Ok(());
    }

    if !output.is_json() && !output.has_template() {
        println!(
            "{} Updating priority to '{}' for {} issues...",
            ">>".cyan(),
            priority,
            issues.len()
        );
    }

    if !(0..=4).contains(&priority) {
        if output.is_json() || output.has_template() {
            print_json(
                &json!({ "error": "Priority must be between 0 and 4", "results": [] }),
                output,
            )?;
        } else {
            println!("Priority must be between 0 and 4.");
        }
        return Ok(());
    }

    let client = LinearClient::new()?;
    let (infos, mut failures) = resolve_issue_infos(&client, &issues).await;
    let mut updated = batch_update_infos(&client, &infos, json!({ "priority": priority })).await;
    failures.append(&mut updated);
    print_summary(&failures, "priority updated", output);

    Ok(())
}

async fn bulk_move_project(
    project: &str,
    issues: Vec<String>,
    output: &OutputOptions,
) -> Result<()> {
    if !ensure_issues_present(&issues, output)? {
        return Ok(());
    }

    if !output.is_json() && !output.has_template() {
        println!(
            "{} Moving {} issues to project '{}'...",
            ">>".cyan(),
            issues.len(),
            project
        );
    }

    let client = LinearClient::new()?;
    let project_id = match resolve_project_id(&client, project, true).await {
        Ok(id) => id,
        Err(e) => {
            if output.is_json() || output.has_template() {
                print_json(
                    &json!({ "error": format!("Failed to resolve project '{}': {}", project, e), "results": [] }),
                    output,
                )?;
            } else {
                println!(
                    "{} Failed to resolve project '{}': {}",
                    "x".red(),
                    project,
                    e
                );
            }
            return Ok(());
        }
    };

    let (infos, mut failures) = resolve_issue_infos(&client, &issues).await;
    let mut updated = batch_update_infos(&client, &infos, json!({ "projectId": project_id })).await;
    failures.append(&mut updated);
    print_summary(&failures, "moved to project", output);

    Ok(())
}

async fn bulk_move_cycle(cycle: &str, issues: Vec<String>, output: &OutputOptions) -> Result<()> {
    if !ensure_issues_present(&issues, output)? {
        return Ok(());
    }

    if !output.is_json() && !output.has_template() {
        println!(
            "{} Moving {} issues to cycle '{}'...",
            ">>".cyan(),
            issues.len(),
            cycle
        );
    }

    let client = LinearClient::new()?;
    let (infos, mut failures) = resolve_issue_infos(&client, &issues).await;

    let mut grouped: HashMap<String, Vec<IssueInfo>> = HashMap::new();
    for info in infos {
        grouped.entry(info.team_id.clone()).or_default().push(info);
    }

    let mut successes = Vec::new();
    for (team_id, team_infos) in grouped {
        match resolve_cycle_id(&client, &team_id, cycle).await {
            Ok(cycle_id) => {
                let mut team_results =
                    batch_update_infos(&client, &team_infos, json!({ "cycleId": cycle_id })).await;
                successes.append(&mut team_results);
            }
            Err(e) => {
                for info in team_infos {
                    failures.push(BulkResult {
                        issue_id: info.issue_id,
                        success: false,
                        identifier: info.identifier,
                        error: Some(e.to_string()),
                    });
                }
            }
        }
    }

    failures.extend(successes);
    print_summary(&failures, "moved to cycle", output);
    Ok(())
}

async fn bulk_archive(issues: Vec<String>, output: &OutputOptions) -> Result<()> {
    if !ensure_issues_present(&issues, output)? {
        return Ok(());
    }

    if !output.is_json() && !output.has_template() {
        println!("{} Archiving {} issues...", ">>".cyan(), issues.len());
    }

    let client = LinearClient::new()?;
    let futures: Vec<_> = issues
        .iter()
        .map(|issue_id| {
            let client = &client;
            let id = issue_id.clone();
            async move { archive_issue(client, &id).await }
        })
        .collect();

    let results = join_all(futures).await;
    print_summary(&results, "archived", output);

    Ok(())
}

async fn bulk_create(data: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let mut issues = parse_batch_create_issues(data)?;

    for issue in &mut issues {
        normalize_issue_create_input(&client, issue, output).await?;
    }

    if output.dry_run {
        let payload = json!({
            "dry_run": true,
            "count": issues.len(),
            "input": { "issues": issues },
        });
        if output.is_json() || output.has_template() {
            print_json(&payload, output)?;
        } else {
            println!(
                "{} [DRY RUN] Would create {} issues",
                ">>".cyan(),
                payload["count"]
            );
        }
        return Ok(());
    }

    let mutation = r#"
        mutation($input: IssueBatchCreateInput!) {
            issueBatchCreate(input: $input) {
                success
                issues {
                    id
                    identifier
                    title
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "input": { "issues": issues } })))
        .await?;

    if result["data"]["issueBatchCreate"]["success"].as_bool() != Some(true) {
        anyhow::bail!("Batch create failed");
    }

    let created = result["data"]["issueBatchCreate"]["issues"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    if output.is_json() || output.has_template() {
        print_json(
            &json!({
                "success": true,
                "issues": created,
                "summary": {
                    "created": created.len(),
                }
            }),
            output,
        )?;
        return Ok(());
    }

    if created.is_empty() {
        println!("No issues created.");
        return Ok(());
    }

    println!("{} Created {} issues", "+".green(), created.len());
    for issue in &created {
        let identifier = issue["identifier"].as_str().unwrap_or("-");
        let title = issue["title"].as_str().unwrap_or("");
        println!(
            "  {} {}",
            identifier.cyan(),
            truncate(title, display_options().max_width(70))
        );
    }

    Ok(())
}

fn parse_batch_create_issues(data: &str) -> Result<Vec<Value>> {
    let raw = if data == "-" {
        let stdin = io::stdin();
        let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();
        lines.join("\n")
    } else if let Some(path) = data.strip_prefix('@') {
        std::fs::read_to_string(path)?
    } else if Path::new(data).exists() {
        std::fs::read_to_string(data)?
    } else {
        data.to_string()
    };

    let parsed: Value =
        serde_json::from_str(&raw).map_err(|e| anyhow::anyhow!("Invalid JSON payload: {}", e))?;

    let issues = if let Some(array) = parsed.as_array() {
        array.clone()
    } else if let Some(array) = parsed.get("issues").and_then(|v| v.as_array()) {
        array.clone()
    } else {
        anyhow::bail!("Expected a JSON array or an object with an 'issues' array.")
    };

    if issues.is_empty() {
        anyhow::bail!("No issues provided in batch payload.");
    }

    for (idx, issue) in issues.iter().enumerate() {
        if !issue.is_object() {
            anyhow::bail!("Issue at index {} is not a JSON object.", idx);
        }
    }

    Ok(issues)
}

async fn normalize_issue_create_input(
    client: &LinearClient,
    issue: &mut Value,
    output: &OutputOptions,
) -> Result<()> {
    let Some(obj) = issue.as_object_mut() else {
        anyhow::bail!("Issue payload entry must be an object.");
    };

    if !obj.contains_key("teamId") {
        if let Some(team_value) = obj.remove("team") {
            if let Some(team) = team_value.as_str() {
                let team_id = resolve_team_id(client, team, &output.cache).await?;
                obj.insert("teamId".to_string(), json!(team_id));
            }
        }
    }

    if !obj.contains_key("assigneeId") {
        if let Some(assignee_value) = obj.remove("assignee") {
            if let Some(assignee) = assignee_value.as_str() {
                let assignee_id = resolve_user_id(client, assignee).await?;
                obj.insert("assigneeId".to_string(), json!(assignee_id));
            }
        }
    }

    if !obj.contains_key("projectId") {
        if let Some(project_value) = obj.remove("project") {
            if let Some(project) = project_value.as_str() {
                let project_id = resolve_project_id(client, project, true).await?;
                obj.insert("projectId".to_string(), json!(project_id));
            }
        }
    }

    if !obj.contains_key("labelIds") {
        if let Some(labels_value) = obj.remove("labels") {
            if let Some(labels) = labels_value.as_array() {
                let mut label_ids = Vec::new();
                for label in labels {
                    let Some(label_name) = label.as_str() else {
                        anyhow::bail!(
                            "labels entries must be strings when using convenience 'labels'."
                        );
                    };
                    label_ids.push(resolve_label_id(client, label_name).await?);
                }
                obj.insert("labelIds".to_string(), json!(label_ids));
            }
        }
    }

    Ok(())
}

async fn archive_issue(client: &LinearClient, issue_id: &str) -> BulkResult {
    let (uuid, _team_id, identifier) = match get_issue_info(client, issue_id).await {
        Ok(info) => info,
        Err(e) => {
            return BulkResult {
                issue_id: issue_id.to_string(),
                success: false,
                identifier: None,
                error: Some(e.to_string()),
            };
        }
    };

    let mutation = r#"
        mutation($id: String!) {
            issueArchive(id: $id) {
                success
                entity { identifier }
            }
        }
    "#;

    match client.mutate(mutation, Some(json!({ "id": uuid }))).await {
        Ok(result) => {
            if result["data"]["issueArchive"]["success"].as_bool() == Some(true) {
                let identifier = result["data"]["issueArchive"]["entity"]["identifier"]
                    .as_str()
                    .map(|s| s.to_string())
                    .or(identifier);
                BulkResult {
                    issue_id: issue_id.to_string(),
                    success: true,
                    identifier,
                    error: None,
                }
            } else {
                BulkResult {
                    issue_id: issue_id.to_string(),
                    success: false,
                    identifier,
                    error: Some("Archive failed".to_string()),
                }
            }
        }
        Err(e) => BulkResult {
            issue_id: issue_id.to_string(),
            success: false,
            identifier,
            error: Some(e.to_string()),
        },
    }
}

fn print_summary(results: &[BulkResult], action: &str, output: &OutputOptions) {
    let success_count = results.iter().filter(|r| r.success).count();
    let failure_count = results.len() - success_count;
    let id_width = display_options().max_width(30);
    let err_width = display_options().max_width(60);

    if output.is_json() || output.has_template() {
        let json_results: Vec<_> = results
            .iter()
            .map(|r| {
                json!({
                    "issue_id": r.issue_id,
                    "identifier": r.identifier,
                    "success": r.success,
                    "error": r.error,
                })
            })
            .collect();

        let payload = json!({
            "action": action,
            "results": json_results,
            "summary": {
                "total": results.len(),
                "succeeded": success_count,
                "failed": failure_count,
            }
        });
        if let Err(err) = print_json(&payload, output) {
            eprintln!("Error: {}", err);
        }
        return;
    }

    println!();

    for result in results {
        if result.success {
            let display_id = result.identifier.as_deref().unwrap_or(&result.issue_id);
            let display_id = truncate(display_id, id_width);
            println!("  {} {} {}", "+".green(), display_id.cyan(), action);
        } else {
            let error_msg = result.error.as_deref().unwrap_or("Unknown error");
            let error_msg = truncate(error_msg, err_width);
            println!(
                "  {} {} failed: {}",
                "x".red(),
                result.issue_id.cyan(),
                error_msg.dimmed()
            );
        }
    }

    println!();
    println!(
        "{} Summary: {} succeeded, {} failed",
        ">>".cyan(),
        success_count.to_string().green(),
        if failure_count > 0 {
            failure_count.to_string().red().to_string()
        } else {
            failure_count.to_string()
        }
    );
}
