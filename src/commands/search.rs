use anyhow::Result;
use clap::Subcommand;
use serde_json::json;
use tabled::{Table, Tabled};

use crate::api::{resolve_team_id, LinearClient};
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
    /// Search documents by query string
    Documents {
        /// Search query string
        query: String,
        /// Include archived documents
        #[arg(short, long)]
        archived: bool,
        /// Include comments in search ranking
        #[arg(long)]
        include_comments: bool,
        /// Restrict to a team key/name/ID
        #[arg(short, long)]
        team: Option<String>,
    },
    /// Semantic search across issues/projects/initiatives/documents
    Semantic {
        /// Search query string
        query: String,
        /// Entity types to include (issue,project,initiative,document)
        #[arg(long, value_delimiter = ',')]
        types: Vec<String>,
        /// Maximum number of results
        #[arg(long)]
        max_results: Option<i32>,
        /// Include archived entities
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

#[derive(Tabled)]
struct DocumentRow {
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "Team")]
    team: String,
    #[tabled(rename = "Linked")]
    linked_to: String,
    #[tabled(rename = "Updated")]
    updated_at: String,
    #[tabled(rename = "ID")]
    id: String,
}

#[derive(Tabled)]
struct SemanticRow {
    #[tabled(rename = "Type")]
    entity_type: String,
    #[tabled(rename = "Ref")]
    reference: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "ID")]
    id: String,
}

pub async fn handle(cmd: SearchCommands, output: &OutputOptions) -> Result<()> {
    match cmd {
        SearchCommands::Issues { query, archived } => search_issues(&query, archived, output).await,
        SearchCommands::Projects { query, archived } => {
            search_projects(&query, archived, output).await
        }
        SearchCommands::Documents {
            query,
            archived,
            include_comments,
            team,
        } => search_documents(&query, archived, include_comments, team, output).await,
        SearchCommands::Semantic {
            query,
            types,
            max_results,
            archived,
        } => search_semantic(&query, &types, max_results, archived, output).await,
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

async fn search_documents(
    query: &str,
    include_archived: bool,
    include_comments: bool,
    team: Option<String>,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;
    let team_id = if let Some(team) = team {
        Some(resolve_team_id(&client, &team, &output.cache).await?)
    } else {
        None
    };

    let graphql_query = r#"
        query(
            $term: String!,
            $includeArchived: Boolean,
            $includeComments: Boolean,
            $teamId: String,
            $first: Int,
            $after: String,
            $last: Int,
            $before: String
        ) {
            searchDocuments(
                term: $term,
                includeArchived: $includeArchived,
                includeComments: $includeComments,
                teamId: $teamId,
                first: $first,
                after: $after,
                last: $last,
                before: $before
            ) {
                nodes {
                    id
                    title
                    url
                    updatedAt
                    team { key }
                    project { name }
                    issue { identifier }
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
    variables.insert("term".to_string(), json!(query));
    variables.insert("includeArchived".to_string(), json!(include_archived));
    variables.insert("includeComments".to_string(), json!(include_comments));
    variables.insert("teamId".to_string(), json!(team_id));

    let pagination = output.pagination.with_default_limit(50);
    let mut documents = paginate_nodes(
        &client,
        graphql_query,
        variables,
        &["data", "searchDocuments", "nodes"],
        &["data", "searchDocuments", "pageInfo"],
        &pagination,
        50,
    )
    .await?;

    if output.is_json() || output.has_template() {
        print_json(&serde_json::json!(documents), output)?;
        return Ok(());
    }

    filter_values(&mut documents, &output.filters);
    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut documents, sort_key, output.json.order);
    }

    ensure_non_empty(&documents, output)?;
    if documents.is_empty() {
        println!("No documents found matching: {}", query);
        return Ok(());
    }

    let title_width = display_options().max_width(40);
    let linked_width = display_options().max_width(28);
    let rows: Vec<DocumentRow> = documents
        .iter()
        .map(|d| {
            let linked_to = if let Some(identifier) = d["issue"]["identifier"].as_str() {
                identifier.to_string()
            } else if let Some(project_name) = d["project"]["name"].as_str() {
                project_name.to_string()
            } else {
                "-".to_string()
            };
            DocumentRow {
                title: truncate(d["title"].as_str().unwrap_or(""), title_width),
                team: d["team"]["key"].as_str().unwrap_or("-").to_string(),
                linked_to: truncate(&linked_to, linked_width),
                updated_at: d["updatedAt"]
                    .as_str()
                    .unwrap_or("-")
                    .chars()
                    .take(10)
                    .collect(),
                id: d["id"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} documents found", documents.len());
    Ok(())
}

async fn search_semantic(
    query: &str,
    types: &[String],
    max_results: Option<i32>,
    include_archived: bool,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;

    for t in types {
        let valid = matches!(t.as_str(), "issue" | "project" | "initiative" | "document");
        if !valid {
            anyhow::bail!(
                "Invalid semantic type '{}'. Use issue, project, initiative, document.",
                t
            );
        }
    }

    let graphql_query = r#"
        query(
            $query: String!,
            $types: [SemanticSearchResultType!],
            $maxResults: Int,
            $includeArchived: Boolean
        ) {
            semanticSearch(
                query: $query,
                types: $types,
                maxResults: $maxResults,
                includeArchived: $includeArchived
            ) {
                results {
                    id
                    type
                    issue { id identifier title }
                    project { id name }
                    initiative { id name }
                    document { id title }
                }
            }
        }
    "#;

    let mut variables = serde_json::Map::new();
    variables.insert("query".to_string(), json!(query));
    variables.insert(
        "types".to_string(),
        if types.is_empty() {
            serde_json::Value::Null
        } else {
            json!(types)
        },
    );
    variables.insert(
        "maxResults".to_string(),
        json!(max_results.unwrap_or(output.pagination.limit.unwrap_or(20) as i32)),
    );
    variables.insert("includeArchived".to_string(), json!(include_archived));

    let result = client.query(graphql_query, Some(json!(variables))).await?;
    let mut entries = result["data"]["semanticSearch"]["results"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    if output.is_json() || output.has_template() {
        print_json(&serde_json::json!(entries), output)?;
        return Ok(());
    }

    filter_values(&mut entries, &output.filters);
    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut entries, sort_key, output.json.order);
    }

    ensure_non_empty(&entries, output)?;
    if entries.is_empty() {
        println!("No semantic matches found for: {}", query);
        return Ok(());
    }

    let title_width = display_options().max_width(50);
    let rows: Vec<SemanticRow> = entries
        .iter()
        .map(|entry| {
            let entity_type = entry["type"].as_str().unwrap_or("unknown").to_string();
            let (reference, title) = match entity_type.as_str() {
                "issue" => (
                    entry["issue"]["identifier"]
                        .as_str()
                        .unwrap_or("-")
                        .to_string(),
                    entry["issue"]["title"].as_str().unwrap_or("-").to_string(),
                ),
                "project" => (
                    "project".to_string(),
                    entry["project"]["name"].as_str().unwrap_or("-").to_string(),
                ),
                "initiative" => (
                    "initiative".to_string(),
                    entry["initiative"]["name"]
                        .as_str()
                        .unwrap_or("-")
                        .to_string(),
                ),
                "document" => (
                    "document".to_string(),
                    entry["document"]["title"]
                        .as_str()
                        .unwrap_or("-")
                        .to_string(),
                ),
                _ => ("-".to_string(), "-".to_string()),
            };
            SemanticRow {
                entity_type,
                reference,
                title: truncate(&title, title_width),
                id: entry["id"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} semantic results", entries.len());
    Ok(())
}
