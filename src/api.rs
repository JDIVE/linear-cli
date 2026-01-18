use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::{json, Value};

use crate::config;
use crate::error::CliError;

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

fn is_uuid(value: &str) -> bool {
    value.len() == 36 && value.chars().filter(|c| *c == '-').count() == 4
}

fn is_issue_identifier(value: &str) -> bool {
    let mut parts = value.splitn(2, '-');
    let prefix = parts.next().unwrap_or("");
    let number = parts.next().unwrap_or("");
    if prefix.is_empty() || number.is_empty() {
        return false;
    }
    if !prefix.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    number.chars().all(|c| c.is_ascii_digit())
}

/// Resolves a team key (like "SCW") or name to a team UUID.
/// If the input is already a UUID (36 characters with dashes), returns it as-is.
pub async fn resolve_team_id(client: &LinearClient, team: &str) -> Result<String> {
    // If already a UUID (36 chars with dashes pattern), return as-is
    if is_uuid(team) {
        return Ok(team.to_string());
    }

    // Query to find team by key or name
    let query = r#"
        query {
            teams(first: 100) {
                nodes {
                    id
                    key
                    name
                }
            }
        }
    "#;

    let result = client.query(query, None).await?;
    let empty = vec![];
    let teams = result["data"]["teams"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    // First try exact key match (case-insensitive)
    if let Some(team_data) = teams
        .iter()
        .find(|t| t["key"].as_str().map(|k| k.eq_ignore_ascii_case(team)) == Some(true))
    {
        if let Some(id) = team_data["id"].as_str() {
            return Ok(id.to_string());
        }
    }

    // Then try exact name match (case-insensitive)
    if let Some(team_data) = teams
        .iter()
        .find(|t| t["name"].as_str().map(|n| n.eq_ignore_ascii_case(team)) == Some(true))
    {
        if let Some(id) = team_data["id"].as_str() {
            return Ok(id.to_string());
        }
    }

    anyhow::bail!(
        "Team not found: '{}'. Use 'linear-cli t list' to see available teams.",
        team
    )
}

/// Resolve an issue identifier (e.g., "ENG-123") or UUID to a UUID.
pub async fn resolve_issue_id(
    client: &LinearClient,
    issue: &str,
    include_archived: bool,
) -> Result<String> {
    if is_uuid(issue) {
        return Ok(issue.to_string());
    }

    if !is_issue_identifier(issue) {
        anyhow::bail!(
            "Invalid issue identifier '{}'. Use an identifier like ENG-123 or a UUID.",
            issue
        );
    }

    let query = r#"
        query($term: String!, $includeArchived: Boolean, $first: Int, $after: String) {
            searchIssues(term: $term, includeArchived: $includeArchived, first: $first, after: $after) {
                nodes { id identifier }
                pageInfo {
                    hasNextPage
                    endCursor
                }
            }
        }
    "#;

    let max_pages = 20;
    let mut include = include_archived;
    for _ in 0..2 {
        let mut after: Option<String> = None;
        let mut pages = 0usize;
        loop {
            if pages >= max_pages {
                break;
            }
            pages += 1;
            let result = client
                .query(
                    query,
                    Some(json!({
                        "term": issue,
                        "includeArchived": include,
                        "first": 50,
                        "after": after
                    })),
                )
                .await?;

            let empty = vec![];
            let nodes = result["data"]["searchIssues"]["nodes"]
                .as_array()
                .unwrap_or(&empty);

            for node in nodes {
                let identifier = node["identifier"].as_str().unwrap_or("");
                if identifier.eq_ignore_ascii_case(issue) {
                    if let Some(id) = node["id"].as_str() {
                        return Ok(id.to_string());
                    }
                }
            }

            let page_info = &result["data"]["searchIssues"]["pageInfo"];
            let has_next = page_info["hasNextPage"].as_bool().unwrap_or(false);
            if !has_next {
                break;
            }

            after = page_info["endCursor"]
                .as_str()
                .map(|s| s.to_string());
            if after.is_none() {
                break;
            }
        }

        if pages >= max_pages {
            anyhow::bail!(
                "Issue not found after scanning {} results. Provide an issue ID or UUID.",
                max_pages * 50
            );
        }

        if include {
            break;
        }
        include = true;
    }

    anyhow::bail!("Issue not found: {}", issue)
}

/// Resolve a project name or UUID to a UUID.
pub async fn resolve_project_id(
    client: &LinearClient,
    project: &str,
    include_archived: bool,
) -> Result<String> {
    if is_uuid(project) {
        return Ok(project.to_string());
    }

    let query = r#"
        query($name: String!, $includeArchived: Boolean) {
            projects(
                first: 1,
                includeArchived: $includeArchived,
                filter: { name: { eqIgnoreCase: $name } }
            ) {
                nodes { id name }
            }
        }
    "#;

    let mut include = include_archived;
    for _ in 0..2 {
        let result = client
            .query(
                query,
                Some(json!({
                    "name": project,
                    "includeArchived": include
                })),
            )
            .await?;

        let empty = vec![];
        let nodes = result["data"]["projects"]["nodes"]
            .as_array()
            .unwrap_or(&empty);

        if let Some(node) = nodes.first() {
            if let Some(id) = node["id"].as_str() {
                return Ok(id.to_string());
            }
        }

        if include {
            break;
        }
        include = true;
    }

    anyhow::bail!("Project not found: {}", project)
}

/// Resolve a workflow state name or UUID to a UUID for a team.
pub async fn resolve_state_id(client: &LinearClient, team_id: &str, state: &str) -> Result<String> {
    if is_uuid(state) {
        return Ok(state.to_string());
    }

    let query = r#"
        query($teamId: String!) {
            team(id: $teamId) {
                states(first: 250) {
                    nodes { id name }
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
        if s["name"]
            .as_str()
            .map(|n| n.eq_ignore_ascii_case(state))
            == Some(true)
        {
            if let Some(id) = s["id"].as_str() {
                return Ok(id.to_string());
            }
        }
    }

    anyhow::bail!("State not found: {}", state)
}

/// Resolve label names or UUIDs to UUIDs for a team.
pub async fn resolve_label_ids(
    client: &LinearClient,
    team_id: &str,
    labels: &[String],
) -> Result<Vec<String>> {
    if labels.is_empty() {
        return Ok(vec![]);
    }

    let query = r#"
        query($teamId: String!) {
            team(id: $teamId) {
                labels(first: 250) {
                    nodes { id name }
                }
            }
        }
    "#;

    let result = client
        .query(query, Some(json!({ "teamId": team_id })))
        .await?;

    let empty = vec![];
    let available = result["data"]["team"]["labels"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    let mut resolved = Vec::with_capacity(labels.len());
    for label in labels {
        if is_uuid(label) {
            resolved.push(label.to_string());
            continue;
        }

        let found = available.iter().find(|l| {
            l["name"]
                .as_str()
                .map(|n| n.eq_ignore_ascii_case(label))
                == Some(true)
        });

        if let Some(l) = found {
            if let Some(id) = l["id"].as_str() {
                resolved.push(id.to_string());
                continue;
            }
        }

        anyhow::bail!("Label not found: {}", label);
    }

    Ok(resolved)
}

/// Resolve a user name/email/UUID to a UUID. Supports "me".
pub async fn resolve_user_id(client: &LinearClient, user: &str) -> Result<String> {
    if user.eq_ignore_ascii_case("me") {
        let query = r#"
            query {
                viewer { id }
            }
        "#;
        let result = client.query(query, None).await?;
        let id = result["data"]["viewer"]["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Could not fetch current user ID"))?;
        return Ok(id.to_string());
    }

    if is_uuid(user) {
        return Ok(user.to_string());
    }

    let query = r#"
        query {
            users(first: 250) {
                nodes { id name email }
            }
        }
    "#;

    let result = client.query(query, None).await?;
    let empty = vec![];
    let users = result["data"]["users"]["nodes"].as_array().unwrap_or(&empty);

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

/// Resolve a project status name or type to a UUID.
pub async fn resolve_project_status_id(client: &LinearClient, status: &str) -> Result<String> {
    if is_uuid(status) {
        return Ok(status.to_string());
    }

    let query = r#"
        query {
            projectStatuses(first: 100) {
                nodes { id name type }
            }
        }
    "#;

    let result = client.query(query, None).await?;
    let empty = vec![];
    let statuses = result["data"]["projectStatuses"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    let status_lower = status.to_lowercase();
    let mut found = statuses.iter().find(|s| {
        s["type"]
            .as_str()
            .map(|t| t.eq_ignore_ascii_case(&status_lower))
            == Some(true)
    });

    if found.is_none() {
        found = statuses.iter().find(|s| {
            s["name"]
                .as_str()
                .map(|n| n.eq_ignore_ascii_case(&status_lower))
                == Some(true)
        });
    }

    if let Some(s) = found {
        if let Some(id) = s["id"].as_str() {
            return Ok(id.to_string());
        }
    }

    anyhow::bail!("Project status not found: {}", status)
}

#[derive(Clone)]
pub struct LinearClient {
    client: Client,
    api_key: String,
}

impl LinearClient {
    pub fn new() -> Result<Self> {
        let api_key = config::get_api_key()?;
        Ok(Self {
            client: Client::new(),
            api_key,
        })
    }

    pub fn with_api_key(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    pub async fn query(&self, query: &str, variables: Option<Value>) -> Result<Value> {
        let body = match variables {
            Some(vars) => json!({ "query": query, "variables": vars }),
            None => json!({ "query": query }),
        };

        let response = self
            .client
            .post(LINEAR_API_URL)
            .header("Content-Type", "application/json")
            .header("Authorization", &self.api_key)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let headers = response.headers().clone();
        let result: Value = response.json().await?;

        if !status.is_success() {
            let retry_after = headers
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let request_id = headers
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_string());
            let details = json!({
                "status": status.as_u16(),
                "reason": status.canonical_reason().unwrap_or("Unknown error"),
                "request_id": request_id,
            });
            let err = match status.as_u16() {
                401 => CliError::new(3, "Authentication failed - check your API key"),
                403 => CliError::new(3, "Access denied - insufficient permissions"),
                404 => CliError::new(2, "Resource not found"),
                429 => CliError::new(4, "Rate limit exceeded").with_retry_after(retry_after),
                _ => CliError::new(
                    1,
                    format!(
                        "HTTP {} {}",
                        status.as_u16(),
                        details["reason"].as_str().unwrap_or("Unknown error")
                    ),
                ),
            };
            return Err(err.with_details(details).into());
        }

        if let Some(errors) = result.get("errors") {
            return Err(CliError::new(1, "GraphQL error")
                .with_details(errors.clone())
                .into());
        }

        Ok(result)
    }

    pub async fn mutate(&self, mutation: &str, variables: Option<Value>) -> Result<Value> {
        self.query(mutation, variables).await
    }

    /// Fetch raw bytes from a URL with authorization header (for Linear uploads)
    pub async fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .client
            .get(url)
            .header("Authorization", &self.api_key)
            .send()
            .await
            .context("Failed to connect to Linear uploads")?;

        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let request_id = response
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_string());
            let details = json!({
                "status": status.as_u16(),
                "reason": status.canonical_reason().unwrap_or("Unknown error"),
                "request_id": request_id,
            });
            let err = match status.as_u16() {
                401 => CliError::new(3, "Authentication failed - check your API key"),
                403 => CliError::new(3, "Access denied to this upload"),
                404 => CliError::new(2, "Upload not found"),
                429 => CliError::new(4, "Rate limit exceeded").with_retry_after(retry_after),
                _ => CliError::new(
                    1,
                    format!(
                        "HTTP {} {}",
                        status.as_u16(),
                        details["reason"].as_str().unwrap_or("Unknown error")
                    ),
                ),
            };
            return Err(err.with_details(details).into());
        }

        let bytes: Vec<u8> = response
            .bytes()
            .await
            .context("Failed to read response body")?
            .to_vec();
        Ok(bytes)
    }
}
