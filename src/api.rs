use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::{json, Value};

use crate::config;

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

/// Resolves a team key (like "SCW") or name to a team UUID.
/// If the input is already a UUID (36 characters with dashes), returns it as-is.
pub async fn resolve_team_id(client: &LinearClient, team: &str) -> Result<String> {
    // If already a UUID (36 chars with dashes pattern), return as-is
    if team.len() == 36 && team.chars().filter(|c| *c == '-').count() == 4 {
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

        let result: Value = response.json().await?;

        if let Some(errors) = result.get("errors") {
            anyhow::bail!("GraphQL error: {}", errors);
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
            let error_msg = match status.as_u16() {
                401 => "Authentication failed - check your API key".to_string(),
                403 => "Access denied to this upload".to_string(),
                404 => "Upload not found".to_string(),
                _ => format!(
                    "HTTP {} {}",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("Unknown error")
                ),
            };
            anyhow::bail!("{}", error_msg);
        }

        let bytes: Vec<u8> = response
            .bytes()
            .await
            .context("Failed to read response body")?
            .to_vec();
        Ok(bytes)
    }
}
