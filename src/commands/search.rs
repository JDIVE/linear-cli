use anyhow::Result;
use clap::Subcommand;
use serde_json::json;
use tabled::{Table, Tabled};

use crate::api::LinearClient;
use crate::display_options;
use crate::output::{ensure_non_empty, filter_values, print_json, sort_values, OutputOptions};
use crate::pagination::paginate_nodes;
use crate::text::truncate;

#[derive(Subcommand)]
pub enum SearchCommands {
    /// Search issues by query string
    Issues {
        /// Search query string
        query: String,
        /// Include archived issues
        #[arg(short, long)]
        archived: bool,
    },
    /// Search projects by query string
    Projects {
        /// Search query string
        query: String,
        /// Include archived projects
        #[arg(short, long)]
        archived: bool,
    },
}

#[derive(Tabled)]
struct IssueRow {
    #[tabled(rename = "Identifier")]
    identifier: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "State")]
    state: String,
    #[tabled(rename = "Priority")]
    priority: String,
    #[tabled(rename = "ID")]
    id: String,
}

#[derive(Tabled)]
struct ProjectRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Labels")]
    labels: String,
    #[tabled(rename = "ID")]
    id: String,
}

pub async fn handle(cmd: SearchCommands, output: &OutputOptions) -> Result<()> {
    match cmd {
        SearchCommands::Issues { query, archived } => search_issues(&query, archived, output).await,
        SearchCommands::Projects { query, archived } => {
            search_projects(&query, archived, output).await
        }
    }
}

async fn search_issues(query: &str, include_archived: bool, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    let graphql_query = r#"
        query($first: Int, $after: String, $last: Int, $before: String, $includeArchived: Boolean, $filter: IssueFilter) {
            issues(first: $first, after: $after, last: $last, before: $before, includeArchived: $includeArchived, filter: $filter) {
                nodes {
                    id
                    identifier
                    title
                    priority
                    state { name }
                }
                pageInfo {
                    hasNextPage
                    endCursor
                    hasPreviousPage
                    startCursor
                }
            }
        }
    "#;

    let mut variables = serde_json::Map::new();
    variables.insert("includeArchived".to_string(), json!(include_archived));
    variables.insert(
        "filter".to_string(),
        json!({
            "or": [
                { "title": { "containsIgnoreCase": query } },
                { "description": { "containsIgnoreCase": query } }
            ]
        }),
    );

    let pagination = output.pagination.with_default_limit(50);
    let mut issues = paginate_nodes(
        &client,
        graphql_query,
        variables,
        &["data", "issues", "nodes"],
        &["data", "issues", "pageInfo"],
        &pagination,
        50,
    )
    .await?;

    if output.is_json() || output.has_template() {
        print_json(&serde_json::json!(issues), output)?;
        return Ok(());
    }

    filter_values(&mut issues, &output.filters);
    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut issues, sort_key, output.json.order);
    }

    ensure_non_empty(&issues, output)?;
    if issues.is_empty() {
        println!("No issues found matching: {}", query);
        return Ok(());
    }

    let width = display_options().max_width(50);
    let rows: Vec<IssueRow> = issues
        .iter()
        .map(|issue| {
            let priority = match issue["priority"].as_i64() {
                Some(0) => "-".to_string(),
                Some(1) => "Urgent".to_string(),
                Some(2) => "High".to_string(),
                Some(3) => "Normal".to_string(),
                Some(4) => "Low".to_string(),
                _ => "-".to_string(),
            };

            IssueRow {
                identifier: issue["identifier"].as_str().unwrap_or("").to_string(),
                title: truncate(issue["title"].as_str().unwrap_or(""), width),
                state: issue["state"]["name"].as_str().unwrap_or("-").to_string(),
                priority,
                id: issue["id"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} issues found", issues.len());

    Ok(())
}

async fn search_projects(
    query: &str,
    include_archived: bool,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;

    let graphql_query = r#"
        query($first: Int, $after: String, $last: Int, $before: String, $includeArchived: Boolean, $filter: ProjectFilter) {
            projects(first: $first, after: $after, last: $last, before: $before, includeArchived: $includeArchived, filter: $filter) {
                nodes {
                    id
                    name
                    status { name }
                    labels { nodes { name } }
                }
                pageInfo {
                    hasNextPage
                    endCursor
                    hasPreviousPage
                    startCursor
                }
            }
        }
    "#;

    let mut variables = serde_json::Map::new();
    variables.insert("includeArchived".to_string(), json!(include_archived));
    variables.insert(
        "filter".to_string(),
        json!({
            "name": { "containsIgnoreCase": query }
        }),
    );

    let pagination = output.pagination.with_default_limit(50);
    let mut projects = paginate_nodes(
        &client,
        graphql_query,
        variables,
        &["data", "projects", "nodes"],
        &["data", "projects", "pageInfo"],
        &pagination,
        50,
    )
    .await?;

    if output.is_json() || output.has_template() {
        print_json(&serde_json::json!(projects), output)?;
        return Ok(());
    }

    filter_values(&mut projects, &output.filters);
    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut projects, sort_key, output.json.order);
    }

    ensure_non_empty(&projects, output)?;
    if projects.is_empty() {
        println!("No projects found matching: {}", query);
        return Ok(());
    }

    let name_width = display_options().max_width(40);
    let label_width = display_options().max_width(40);
    let rows: Vec<ProjectRow> = projects
        .iter()
        .map(|p| {
            let labels: Vec<String> = p["labels"]["nodes"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|l| l["name"].as_str().unwrap_or("").to_string())
                .collect();

            ProjectRow {
                name: truncate(p["name"].as_str().unwrap_or(""), name_width),
                status: p["status"]["name"].as_str().unwrap_or("-").to_string(),
                labels: if labels.is_empty() {
                    "-".to_string()
                } else {
                    truncate(&labels.join(", "), label_width)
                },
                id: p["id"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} projects found", projects.len());

    Ok(())
}
