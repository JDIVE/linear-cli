use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate, Utc};
use clap::{Subcommand, ValueEnum};
use colored::Colorize;
use serde_json::{json, Map, Value};
use std::io::{self, BufRead};
use std::process::Command;
use tabled::{Table, Tabled};

use crate::api::{
    resolve_issue_id, resolve_label_ids, resolve_project_id, resolve_state_id, resolve_team_id,
    resolve_user_id, LinearClient,
};
use crate::display_options;
use crate::output::{ensure_non_empty, filter_values, print_json, sort_values, OutputOptions};
use crate::pagination::paginate_nodes;
use crate::text::truncate;
use crate::AgentOptions;

use super::templates;

#[derive(Clone, Copy, ValueEnum)]
pub enum DueFilter {
    Overdue,
    Today,
    #[value(name = "this-week")]
    ThisWeek,
    #[value(name = "next-week")]
    NextWeek,
    #[value(name = "no-due")]
    NoDue,
}

#[derive(Subcommand)]
pub enum IssueCommands {
    /// List issues
    #[command(alias = "ls")]
    #[command(after_help = r#"EXAMPLES:
    linear issues list                         # List all issues
    linear i list -t ENG                       # Filter by team
    linear i list -t ENG -s "In Progress"      # Filter by team and status
    linear i list --assignee me                # Show my assigned issues
    linear i list --project "My Project"       # Filter by project name
    linear i list --output json                # Output as JSON"#)]
    List {
        /// Filter by team name or ID
        #[arg(short, long)]
        team: Option<String>,
        /// Filter by state name or ID
        #[arg(short, long)]
        state: Option<String>,
        /// Filter by assignee (user ID, name, email, or "me")
        #[arg(short, long)]
        assignee: Option<String>,
        /// Filter by project name
        #[arg(long)]
        project: Option<String>,
        /// Filter by label name or ID
        #[arg(long)]
        label: Option<String>,
        /// Filter by cycle name/number or ID
        #[arg(long)]
        cycle: Option<String>,
        /// Filter by initiative name or ID
        #[arg(long)]
        initiative: Option<String>,
        /// Filter by due date shortcut
        #[arg(long, value_enum)]
        due: Option<DueFilter>,
        /// Include archived issues
        #[arg(long)]
        archived: bool,
    },
    /// Get issue details
    #[command(after_help = r#"EXAMPLES:
    linear issues get LIN-123                  # View issue by identifier
    linear i get abc123-uuid                   # View issue by ID
    linear i get LIN-1 LIN-2 LIN-3             # Get multiple issues
    linear i get LIN-123 --output json         # Output as JSON
    echo "LIN-123" | linear i get -            # Read ID from stdin (piping)"#)]
    Get {
        /// Issue ID(s) or identifier(s). Use "-" to read from stdin.
        ids: Vec<String>,
    },
    /// Create a new issue
    #[command(after_help = r#"EXAMPLES:
    linear issues create "Fix bug" -t ENG      # Create with title and team
    linear i create "Feature" -t ENG -p 2      # Create with high priority
    linear i create "Task" -t ENG -a me        # Assign to yourself
    linear i create "Bug" -t ENG --dry-run     # Preview without creating"#)]
    Create {
        /// Issue title
        title: String,
        /// Team name or ID (can be provided via template)
        #[arg(short, long)]
        team: Option<String>,
        /// Issue description (markdown). Use "-" to read from stdin.
        #[arg(short, long)]
        description: Option<String>,
        /// JSON input for issue fields. Use "-" to read from stdin.
        #[arg(long)]
        data: Option<String>,
        /// Priority (0=none, 1=urgent, 2=high, 3=normal, 4=low)
        #[arg(short, long)]
        priority: Option<i32>,
        /// State name or ID
        #[arg(short, long)]
        state: Option<String>,
        /// Assignee (user ID, name, email, or "me")
        #[arg(short, long)]
        assignee: Option<String>,
        /// Labels to add (can be specified multiple times)
        #[arg(short, long)]
        labels: Vec<String>,
        /// Project name or ID
        #[arg(long)]
        project: Option<String>,
        /// Estimate points
        #[arg(long)]
        estimate: Option<i32>,
        /// Due date (YYYY-MM-DD)
        #[arg(long)]
        due: Option<String>,
        /// Parent issue ID or identifier
        #[arg(long)]
        parent: Option<String>,
        /// Template name to use for default values
        #[arg(long)]
        template: Option<String>,
        /// Preview without creating (dry run)
        #[arg(long)]
        dry_run: bool,
    },
    /// Update an existing issue
    #[command(after_help = r#"EXAMPLES:
    linear issues update LIN-123 -s Done       # Mark as done
    linear i update LIN-123 -T "New title"     # Change title
    linear i update LIN-123 -p 1               # Set to urgent priority
    linear i update LIN-123 -a me              # Assign to yourself"#)]
    Update {
        /// Issue ID
        id: String,
        /// New title
        #[arg(short = 'T', long)]
        title: Option<String>,
        /// New description
        #[arg(short, long)]
        description: Option<String>,
        /// JSON input for issue fields. Use "-" to read from stdin.
        #[arg(long)]
        data: Option<String>,
        /// New priority (0=none, 1=urgent, 2=high, 3=normal, 4=low)
        #[arg(short, long)]
        priority: Option<i32>,
        /// New state name or ID
        #[arg(short, long)]
        state: Option<String>,
        /// New assignee (user ID, name, email, or "me")
        #[arg(short, long)]
        assignee: Option<String>,
        /// New labels (replaces existing labels)
        #[arg(short, long)]
        labels: Vec<String>,
        /// New project (name or ID)
        #[arg(long)]
        project: Option<String>,
        /// New estimate points
        #[arg(long)]
        estimate: Option<i32>,
        /// New due date (YYYY-MM-DD)
        #[arg(long)]
        due: Option<String>,
        /// New parent issue ID or identifier
        #[arg(long)]
        parent: Option<String>,
        /// Preview without updating (dry run)
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete an issue
    #[command(after_help = r#"EXAMPLES:
    linear issues delete LIN-123               # Delete with confirmation
    linear i delete LIN-123 --force            # Delete without confirmation"#)]
    Delete {
        /// Issue ID
        id: String,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Start working on an issue (set to In Progress and assign to me)
    #[command(after_help = r#"EXAMPLES:
    linear issues start LIN-123                # Start working on issue
    linear i start LIN-123 --checkout          # Start and checkout git branch
    linear i start LIN-123 -c -b feature/fix   # Start with custom branch"#)]
    Start {
        /// Issue ID or identifier (e.g., "LIN-123")
        id: String,
        /// Checkout a git branch for the issue
        #[arg(short, long)]
        checkout: bool,
        /// Custom branch name (optional, uses issue's branch name by default)
        #[arg(short, long)]
        branch: Option<String>,
    },
    /// Stop working on an issue (return to backlog state)
    #[command(after_help = r#"EXAMPLES:
    linear issues stop LIN-123                 # Stop working on issue
    linear i stop LIN-123 --unassign           # Stop and unassign"#)]
    Stop {
        /// Issue ID or identifier (e.g., "LIN-123")
        id: String,
        /// Unassign the issue
        #[arg(short, long)]
        unassign: bool,
    },
    /// Set a reminder for an issue
    #[command(after_help = r#"EXAMPLES:
    linear issues remind LIN-123 --at 2025-01-30T09:00:00Z
    linear i remind LIN-123 --in 2d"#)]
    Remind {
        /// Issue ID or identifier
        id: String,
        /// Reminder time (ISO 8601)
        #[arg(long, conflicts_with = "in")]
        at: Option<String>,
        /// Reminder delay (e.g., 2d, 3h, 30m, 1h30m)
        #[arg(long = "in", conflicts_with = "at")]
        r#in: Option<String>,
    },
    /// Subscribe to an issue
    #[command(after_help = r#"EXAMPLES:
    linear issues subscribe LIN-123
    linear i subscribe LIN-123 --user me"#)]
    Subscribe {
        /// Issue ID or identifier
        id: String,
        /// User to subscribe (user ID, name, email, or "me")
        #[arg(long)]
        user: Option<String>,
    },
    /// Unsubscribe from an issue
    #[command(after_help = r#"EXAMPLES:
    linear issues unsubscribe LIN-123
    linear i unsubscribe LIN-123 --user me"#)]
    Unsubscribe {
        /// Issue ID or identifier
        id: String,
        /// User to unsubscribe (user ID, name, email, or "me")
        #[arg(long)]
        user: Option<String>,
    },
    /// Archive an issue
    #[command(after_help = r#"EXAMPLE:
    linear issues archive LIN-123"#)]
    Archive {
        /// Issue ID or identifier
        id: String,
    },
    /// Unarchive an issue
    #[command(after_help = r#"EXAMPLE:
    linear issues unarchive LIN-123"#)]
    Unarchive {
        /// Issue ID or identifier
        id: String,
    },
}

#[derive(Tabled)]
struct IssueRow {
    #[tabled(rename = "ID")]
    identifier: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "State")]
    state: String,
    #[tabled(rename = "Priority")]
    priority: String,
    #[tabled(rename = "Assignee")]
    assignee: String,
}

pub async fn handle(
    cmd: IssueCommands,
    output: &OutputOptions,
    agent_opts: AgentOptions,
) -> Result<()> {
    match cmd {
        IssueCommands::List {
            team,
            state,
            assignee,
            project,
            label,
            cycle,
            initiative,
            due,
            archived,
        } => {
            list_issues(
                team, state, assignee, project, label, cycle, initiative, due, archived, output,
                agent_opts,
            )
            .await
        }
        IssueCommands::Get { ids } => {
            // Support reading from stdin if no IDs provided or if "-" is passed
            let final_ids: Vec<String> = if ids.is_empty() || (ids.len() == 1 && ids[0] == "-") {
                // Read from stdin
                let stdin = io::stdin();
                stdin
                    .lock()
                    .lines()
                    .map_while(Result::ok)
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.trim().to_string())
                    .collect()
            } else {
                ids
            };
            if final_ids.is_empty() {
                anyhow::bail!(
                    "No issue IDs provided. Provide IDs as arguments or pipe them via stdin."
                );
            }
            get_issues(&final_ids, output).await
        }
        IssueCommands::Create {
            title,
            team,
            description,
            data,
            priority,
            state,
            assignee,
            labels,
            project,
            estimate,
            due,
            parent,
            template,
            dry_run,
        } => {
            let dry_run = dry_run || output.dry_run || agent_opts.dry_run;
            // Load template if specified
            let tpl = if let Some(ref tpl_name) = template {
                templates::get_template(tpl_name)?
                    .ok_or_else(|| anyhow::anyhow!("Template not found: {}", tpl_name))?
            } else {
                templates::IssueTemplate {
                    name: String::new(),
                    title_prefix: None,
                    description: None,
                    default_priority: None,
                    default_labels: vec![],
                    team: None,
                }
            };

            // Team from CLI arg takes precedence, then template, then error
            let data_json = read_json_data(data.as_deref())?;
            let data_team = data_json.as_ref().and_then(|v| {
                v.get("team")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
            });
            let data_team_id = data_json.as_ref().and_then(|v| {
                v.get("teamId")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
            });
            let final_team = team
                .or(tpl.team.clone())
                .or(data_team)
                .or(data_team_id)
                .ok_or_else(|| {
                    anyhow::anyhow!("--team is required (or use a template with a default team)")
                })?;

            // Build title with optional prefix from template
            let final_title = if let Some(ref prefix) = tpl.title_prefix {
                format!("{} {}", prefix, title)
            } else {
                title
            };

            // Merge template defaults with CLI args (CLI takes precedence)
            // Support reading description from stdin if "-" is passed
            if data.as_deref() == Some("-") && description.as_deref() == Some("-") {
                anyhow::bail!("--data - and --description - cannot both read from stdin");
            }

            let final_description = match description.as_deref() {
                Some("-") => {
                    let stdin = io::stdin();
                    let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();
                    Some(lines.join("\n"))
                }
                Some(d) => Some(d.to_string()),
                None => tpl.description.clone(),
            };
            let final_priority = priority.or(tpl.default_priority);

            // Merge labels: template labels + CLI labels
            let mut final_labels = tpl.default_labels.clone();
            final_labels.extend(labels);

            create_issue(
                &final_title,
                &final_team,
                data_json,
                final_description,
                final_priority,
                state,
                assignee,
                final_labels,
                project,
                estimate,
                due,
                parent,
                output,
                agent_opts,
                dry_run,
            )
            .await
        }
        IssueCommands::Update {
            id,
            title,
            description,
            data,
            priority,
            state,
            assignee,
            labels,
            project,
            estimate,
            due,
            parent,
            dry_run,
        } => {
            let dry_run = dry_run || output.dry_run || agent_opts.dry_run;
            if data.as_deref() == Some("-") && description.as_deref() == Some("-") {
                anyhow::bail!("--data - and --description - cannot both read from stdin");
            }

            let data_json = read_json_data(data.as_deref())?;
            // Support reading description from stdin if "-" is passed
            let final_description = match description.as_deref() {
                Some("-") => {
                    let stdin = io::stdin();
                    let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();
                    Some(lines.join("\n"))
                }
                Some(d) => Some(d.to_string()),
                None => None,
            };
            update_issue(
                &id,
                title,
                final_description,
                data_json,
                priority,
                state,
                assignee,
                labels,
                project,
                estimate,
                due,
                parent,
                dry_run,
                output,
                agent_opts,
            )
            .await
        }
        IssueCommands::Delete { id, force } => delete_issue(&id, force, agent_opts).await,
        IssueCommands::Start {
            id,
            checkout,
            branch,
        } => start_issue(&id, checkout, branch, agent_opts).await,
        IssueCommands::Stop { id, unassign } => stop_issue(&id, unassign, agent_opts).await,
        IssueCommands::Remind { id, at, r#in } => remind_issue(&id, at, r#in, output).await,
        IssueCommands::Subscribe { id, user } => subscribe_issue(&id, user, output).await,
        IssueCommands::Unsubscribe { id, user } => unsubscribe_issue(&id, user, output).await,
        IssueCommands::Archive { id } => archive_issue(&id, output, agent_opts).await,
        IssueCommands::Unarchive { id } => unarchive_issue(&id, output, agent_opts).await,
    }
}

fn priority_to_string(priority: Option<i64>) -> String {
    match priority {
        Some(0) => "-".to_string(),
        Some(1) => "Urgent".red().to_string(),
        Some(2) => "High".yellow().to_string(),
        Some(3) => "Normal".to_string(),
        Some(4) => "Low".dimmed().to_string(),
        _ => "-".to_string(),
    }
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36 && value.chars().filter(|c| *c == '-').count() == 4
}

fn due_filter_to_comparator(due: DueFilter) -> Value {
    let today = Local::now().date_naive();
    let week_start = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    let week_end = week_start + Duration::days(6);
    let next_week_start = week_start + Duration::days(7);
    let next_week_end = next_week_start + Duration::days(6);

    let fmt = |date: NaiveDate| date.format("%Y-%m-%d").to_string();

    match due {
        DueFilter::Overdue => json!({ "lt": fmt(today) }),
        DueFilter::Today => json!({ "eq": fmt(today) }),
        DueFilter::ThisWeek => json!({ "gte": fmt(today), "lte": fmt(week_end) }),
        DueFilter::NextWeek => json!({ "gte": fmt(next_week_start), "lte": fmt(next_week_end) }),
        DueFilter::NoDue => json!({ "null": true }),
    }
}

fn parse_reminder_delay(input: &str) -> Result<Duration> {
    let value = input.trim().to_lowercase();
    if value.is_empty() {
        anyhow::bail!("Invalid delay: empty");
    }

    let mut total_minutes = 0i64;
    let mut current_num = String::new();
    for c in value.chars() {
        if c.is_ascii_digit() {
            current_num.push(c);
        } else if c == 'd' || c == 'h' || c == 'm' {
            let num: i64 = current_num.parse().unwrap_or(0);
            if num == 0 {
                anyhow::bail!("Invalid delay '{}'", input);
            }
            match c {
                'd' => total_minutes += num * 24 * 60,
                'h' => total_minutes += num * 60,
                'm' => total_minutes += num,
                _ => {}
            }
            current_num.clear();
        } else {
            anyhow::bail!("Invalid delay '{}'", input);
        }
    }

    if !current_num.is_empty() {
        let num: i64 = current_num.parse().unwrap_or(0);
        total_minutes += num;
    }

    if total_minutes <= 0 {
        anyhow::bail!("Invalid delay '{}'", input);
    }

    Ok(Duration::minutes(total_minutes))
}

#[allow(clippy::too_many_arguments)]
async fn list_issues(
    team: Option<String>,
    state: Option<String>,
    assignee: Option<String>,
    project: Option<String>,
    label: Option<String>,
    cycle: Option<String>,
    initiative: Option<String>,
    due: Option<DueFilter>,
    include_archived: bool,
    output: &OutputOptions,
    _agent_opts: AgentOptions,
) -> Result<()> {
    let client = LinearClient::new()?;

    let query = r#"
        query($filter: IssueFilter, $includeArchived: Boolean, $first: Int, $after: String, $last: Int, $before: String) {
            issues(
                first: $first,
                after: $after,
                last: $last,
                before: $before,
                includeArchived: $includeArchived,
                filter: $filter
            ) {
                nodes {
                    id
                    identifier
                    title
                    priority
                    dueDate
                    state { name }
                    assignee { name }
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

    let mut variables = Map::new();
    variables.insert("includeArchived".to_string(), json!(include_archived));

    let mut filter = Map::new();
    if let Some(t) = team {
        filter.insert("team".to_string(), json!({ "name": { "eqIgnoreCase": t } }));
    }
    if let Some(s) = state {
        filter.insert(
            "state".to_string(),
            json!({ "name": { "eqIgnoreCase": s } }),
        );
    }
    if let Some(a) = assignee {
        filter.insert(
            "assignee".to_string(),
            json!({ "name": { "eqIgnoreCase": a } }),
        );
    }

    let mut project_filter = Map::new();
    if let Some(p) = project {
        project_filter.insert("name".to_string(), json!({ "eqIgnoreCase": p }));
    }
    if let Some(init) = initiative {
        let initiative_filter = if is_uuid(&init) {
            json!({ "id": { "eq": init } })
        } else {
            json!({ "name": { "eqIgnoreCase": init } })
        };
        project_filter.insert(
            "initiatives".to_string(),
            json!({ "some": initiative_filter }),
        );
    }
    if !project_filter.is_empty() {
        filter.insert("project".to_string(), Value::Object(project_filter));
    }

    if let Some(label) = label {
        let label_filter = if is_uuid(&label) {
            json!({ "id": { "eq": label } })
        } else {
            json!({ "name": { "eqIgnoreCase": label } })
        };
        filter.insert("labels".to_string(), json!({ "some": label_filter }));
    }

    if let Some(cycle) = cycle {
        let cycle_filter = if is_uuid(&cycle) {
            json!({ "id": { "eq": cycle } })
        } else {
            json!({ "name": { "eqIgnoreCase": cycle } })
        };
        filter.insert("cycle".to_string(), cycle_filter);
    }

    if let Some(due) = due {
        filter.insert("dueDate".to_string(), due_filter_to_comparator(due));
    }

    if !filter.is_empty() {
        variables.insert("filter".to_string(), Value::Object(filter));
    }

    let pagination = output.pagination.with_default_limit(50);
    let issues = paginate_nodes(
        &client,
        query,
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

    let mut issues = issues;
    filter_values(&mut issues, &output.filters);

    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut issues, sort_key, output.json.order);
    }

    ensure_non_empty(&issues, output)?;
    if issues.is_empty() {
        println!("No issues found.");
        return Ok(());
    }

    let width = display_options().max_width(50);
    let rows: Vec<IssueRow> = issues
        .iter()
        .map(|issue| IssueRow {
            identifier: issue["identifier"].as_str().unwrap_or("").to_string(),
            title: truncate(issue["title"].as_str().unwrap_or(""), width),
            state: issue["state"]["name"].as_str().unwrap_or("-").to_string(),
            priority: priority_to_string(issue["priority"].as_i64()),
            assignee: issue["assignee"]["name"]
                .as_str()
                .unwrap_or("-")
                .to_string(),
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} issues", issues.len());

    Ok(())
}

/// Get multiple issues (supports batch fetching)
async fn get_issues(ids: &[String], output: &OutputOptions) -> Result<()> {
    // Handle single ID (most common case)
    if ids.len() == 1 {
        return get_issue(&ids[0], output).await;
    }

    let client = LinearClient::new()?;

    // For multiple IDs, fetch them in parallel
    let futures: Vec<_> = ids
        .iter()
        .map(|id| {
            let client = client.clone();
            let id = id.clone();
            async move {
                let query = r#"
                    query($id: String!) {
                        issue(id: $id) {
                            id
                            identifier
                            title
                            description
                            priority
                            dueDate
                            url
                            attachments { nodes { id title url createdAt } }
                            state { name }
                            team { name }
                            assignee { name }
                        }
                    }
                "#;
                let result = client.query(query, Some(json!({ "id": id }))).await;
                (id, result)
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    // JSON output: array of issues
    if output.is_json() || output.has_template() {
        let issues: Vec<_> = results
            .iter()
            .filter_map(|(_, r)| {
                r.as_ref().ok().and_then(|data| {
                    let issue = &data["data"]["issue"];
                    if !issue.is_null() {
                        Some(issue.clone())
                    } else {
                        None
                    }
                })
            })
            .collect();
        print_json(&serde_json::json!(issues), output)?;
        return Ok(());
    }

    // Table output
    for (id, result) in results {
        match result {
            Ok(data) => {
                let issue = &data["data"]["issue"];
                if issue.is_null() {
                    eprintln!("{} Issue not found: {}", "!".yellow(), id);
                } else {
                    let identifier = issue["identifier"].as_str().unwrap_or("");
                    let title = issue["title"].as_str().unwrap_or("");
                    let state = issue["state"]["name"].as_str().unwrap_or("-");
                    let priority = priority_to_string(issue["priority"].as_i64());
                    println!("{} {} [{}] {}", identifier.cyan(), title, state, priority);
                }
            }
            Err(e) => {
                eprintln!("{} Error fetching {}: {}", "!".red(), id, e);
            }
        }
    }

    Ok(())
}

async fn get_issue(id: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    let query = r#"
        query($id: String!) {
            issue(id: $id) {
                id
                identifier
                title
                description
                priority
                dueDate
                url
                createdAt
                updatedAt
                attachments { nodes { id title url createdAt } }
                state { name }
                team { name }
                assignee { name email }
                labels { nodes { name color } }
                project { name }
                parent { identifier title }
            }
        }
    "#;

    let result = client.query(query, Some(json!({ "id": id }))).await?;
    let issue = &result["data"]["issue"];

    if issue.is_null() {
        anyhow::bail!("Issue not found: {}", id);
    }

    // Handle JSON output
    if output.is_json() || output.has_template() {
        print_json(issue, output)?;
        return Ok(());
    }

    let identifier = issue["identifier"].as_str().unwrap_or("");
    let title = issue["title"].as_str().unwrap_or("");
    println!("{} {}", identifier.cyan().bold(), title.bold());
    println!("{}", "-".repeat(60));

    if let Some(desc) = issue["description"].as_str() {
        if !desc.is_empty() {
            println!("\n{}", desc);
            println!();
        }
    }

    println!(
        "State:    {}",
        issue["state"]["name"].as_str().unwrap_or("-")
    );
    println!(
        "Priority: {}",
        priority_to_string(issue["priority"].as_i64())
    );
    println!(
        "Team:     {}",
        issue["team"]["name"].as_str().unwrap_or("-")
    );

    if let Some(assignee) = issue["assignee"]["name"].as_str() {
        let email = issue["assignee"]["email"].as_str().unwrap_or("");
        if !email.is_empty() {
            println!("Assignee: {} ({})", assignee, email.dimmed());
        } else {
            println!("Assignee: {}", assignee);
        }
    } else {
        println!("Assignee: -");
    }

    if let Some(project) = issue["project"]["name"].as_str() {
        println!("Project:  {}", project);
    }

    if let Some(parent) = issue["parent"]["identifier"].as_str() {
        let parent_title = issue["parent"]["title"].as_str().unwrap_or("");
        println!("Parent:   {} {}", parent, parent_title.dimmed());
    }

    let labels = issue["labels"]["nodes"].as_array();
    if let Some(labels) = labels {
        if !labels.is_empty() {
            let label_names: Vec<&str> = labels.iter().filter_map(|l| l["name"].as_str()).collect();
            println!("Labels:   {}", label_names.join(", "));
        }
    }

    println!("\nURL: {}", issue["url"].as_str().unwrap_or("-"));
    println!("ID:  {}", issue["id"].as_str().unwrap_or("-"));

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_issue(
    title: &str,
    team: &str,
    data_json: Option<Value>,
    description: Option<String>,
    priority: Option<i32>,
    state: Option<String>,
    assignee: Option<String>,
    labels: Vec<String>,
    project: Option<String>,
    estimate: Option<i32>,
    due: Option<String>,
    parent: Option<String>,
    output: &OutputOptions,
    agent_opts: AgentOptions,
    dry_run: bool,
) -> Result<()> {
    let client = LinearClient::new()?;

    // Determine the final team (CLI arg takes precedence, then template, then error)
    let final_team = team;

    // Resolve team key/name to UUID
    let team_id = resolve_team_id(&client, final_team).await?;

    // Build the title with optional prefix from template
    let final_title = title.to_string();

    let mut input = match data_json {
        Some(Value::Object(map)) => Value::Object(map),
        Some(_) => anyhow::bail!("--data must be a JSON object"),
        None => json!({}),
    };

    input["title"] = json!(final_title);
    input["teamId"] = json!(team_id);

    // CLI args override template values
    if let Some(ref desc) = description {
        input["description"] = json!(desc);
    }
    if let Some(p) = priority {
        input["priority"] = json!(p);
    }
    if let Some(ref s) = state {
        let state_id = resolve_state_id(&client, &team_id, s).await?;
        input["stateId"] = json!(state_id);
    }
    if let Some(ref a) = assignee {
        let assignee_id = resolve_user_id(&client, a).await?;
        input["assigneeId"] = json!(assignee_id);
    }
    if !labels.is_empty() {
        let resolved = resolve_label_ids(&client, &team_id, &labels).await?;
        // Merge with template labels if present
        let existing: Vec<String> = input["labelIds"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let mut all_labels = existing;
        all_labels.extend(resolved);
        input["labelIds"] = json!(all_labels);
    }
    if let Some(ref p) = project {
        let project_id = resolve_project_id(&client, p, false).await?;
        input["projectId"] = json!(project_id);
    }
    if let Some(points) = estimate {
        input["estimate"] = json!(points);
    }
    if let Some(ref date) = due {
        input["dueDate"] = json!(date);
    }
    if let Some(ref parent_id) = parent {
        let resolved = resolve_issue_id(&client, parent_id, true).await?;
        input["parentId"] = json!(resolved);
    }

    // Dry run: show what would be created without actually creating
    if dry_run {
        if output.is_json() || output.has_template() {
            print_json(
                &json!({
                    "dry_run": true,
                    "would_create": {
                        "title": final_title,
                        "team": final_team,
                        "teamId": team_id,
                        "description": description,
                        "priority": priority,
                        "state": state,
                        "assignee": assignee,
                        "labels": labels,
                        "project": project,
                        "estimate": estimate,
                        "due": due,
                        "parent": parent
                    }
                }),
                output,
            )?;
        } else {
            println!("{}", "[DRY RUN] Would create issue:".yellow().bold());
            println!("  Title:       {}", final_title);
            println!("  Team:        {} ({})", final_team, team_id);
            if let Some(ref desc) = description {
                let preview = if desc.len() > 50 {
                    format!("{}...", &desc[..50])
                } else {
                    desc.clone()
                };
                println!("  Description: {}", preview);
            }
            if let Some(p) = priority {
                println!("  Priority:    {}", p);
            }
            if let Some(ref s) = state {
                println!("  State:       {}", s);
            }
            if let Some(ref a) = assignee {
                println!("  Assignee:    {}", a);
            }
            if !labels.is_empty() {
                println!("  Labels:      {}", labels.join(", "));
            }
            if let Some(ref p) = project {
                println!("  Project:     {}", p);
            }
            if let Some(points) = estimate {
                println!("  Estimate:    {}", points);
            }
            if let Some(ref date) = due {
                println!("  Due:         {}", date);
            }
            if let Some(ref parent_id) = parent {
                println!("  Parent:      {}", parent_id);
            }
        }
        return Ok(());
    }

    let mutation = r#"
        mutation($input: IssueCreateInput!) {
            issueCreate(input: $input) {
                success
                issue {
                    id
                    identifier
                    title
                    url
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "input": input })))
        .await?;

    if result["data"]["issueCreate"]["success"].as_bool() == Some(true) {
        let issue = &result["data"]["issueCreate"]["issue"];
        let identifier = issue["identifier"].as_str().unwrap_or("");

        // --id-only: Just output the identifier for chaining
        if agent_opts.id_only {
            println!("{}", identifier);
            return Ok(());
        }

        // Handle JSON output
        if output.is_json() || output.has_template() {
            print_json(issue, output)?;
            return Ok(());
        }

        // Quiet mode: minimal output
        if agent_opts.quiet {
            println!("{}", identifier);
            return Ok(());
        }

        let issue_title = issue["title"].as_str().unwrap_or("");
        println!(
            "{} Created issue: {} {}",
            "+".green(),
            identifier.cyan(),
            issue_title
        );
        println!("  ID:  {}", issue["id"].as_str().unwrap_or(""));
        println!("  URL: {}", issue["url"].as_str().unwrap_or(""));
    } else {
        anyhow::bail!("Failed to create issue");
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_issue(
    id: &str,
    title: Option<String>,
    description: Option<String>,
    data_json: Option<Value>,
    priority: Option<i32>,
    state: Option<String>,
    assignee: Option<String>,
    labels: Vec<String>,
    project: Option<String>,
    estimate: Option<i32>,
    due: Option<String>,
    parent: Option<String>,
    dry_run: bool,
    output: &OutputOptions,
    agent_opts: AgentOptions,
) -> Result<()> {
    let client = LinearClient::new()?;

    let issue_id = resolve_issue_id(&client, id, true).await?;
    let team_id = get_issue_team_id(&client, &issue_id).await?;

    let mut input = match data_json {
        Some(Value::Object(map)) => Value::Object(map),
        Some(_) => anyhow::bail!("--data must be a JSON object"),
        None => json!({}),
    };

    if let Some(t) = title {
        input["title"] = json!(t);
    }
    if let Some(d) = description {
        input["description"] = json!(d);
    }
    if let Some(p) = priority {
        input["priority"] = json!(p);
    }
    if let Some(s) = state {
        let state_id = resolve_state_id(&client, &team_id, &s).await?;
        input["stateId"] = json!(state_id);
    }
    if let Some(a) = assignee {
        let assignee_id = resolve_user_id(&client, &a).await?;
        input["assigneeId"] = json!(assignee_id);
    }
    if !labels.is_empty() {
        let resolved = resolve_label_ids(&client, &team_id, &labels).await?;
        input["labelIds"] = json!(resolved);
    }
    if let Some(p) = project {
        let project_id = resolve_project_id(&client, &p, false).await?;
        input["projectId"] = json!(project_id);
    }
    if let Some(points) = estimate {
        input["estimate"] = json!(points);
    }
    if let Some(date) = due {
        input["dueDate"] = json!(date);
    }
    if let Some(parent_id) = parent {
        let resolved = resolve_issue_id(&client, &parent_id, true).await?;
        input["parentId"] = json!(resolved);
    }

    if input.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        if !agent_opts.quiet {
            println!("No updates specified.");
        }
        return Ok(());
    }

    if dry_run {
        if output.is_json() || output.has_template() {
            print_json(
                &json!({
                    "dry_run": true,
                    "would_update": {
                        "id": id,
                        "input": input,
                    }
                }),
                output,
            )?;
        } else {
            println!("{}", "[DRY RUN] Would update issue:".yellow().bold());
            println!("  ID: {}", id);
        }
        return Ok(());
    }

    let mutation = r#"
        mutation($id: String!, $input: IssueUpdateInput!) {
            issueUpdate(id: $id, input: $input) {
                success
                issue {
                    identifier
                    title
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": issue_id, "input": input })))
        .await?;

    if result["data"]["issueUpdate"]["success"].as_bool() == Some(true) {
        let issue = &result["data"]["issueUpdate"]["issue"];
        let identifier = issue["identifier"].as_str().unwrap_or("");

        // --id-only: Just output the identifier
        if agent_opts.id_only {
            println!("{}", identifier);
            return Ok(());
        }

        // Handle JSON output
        if output.is_json() || output.has_template() {
            print_json(issue, output)?;
            return Ok(());
        }

        // Quiet mode
        if agent_opts.quiet {
            println!("{}", identifier);
            return Ok(());
        }

        println!(
            "{} Updated issue: {} {}",
            "+".green(),
            identifier,
            issue["title"].as_str().unwrap_or("")
        );
    } else {
        anyhow::bail!("Failed to update issue");
    }

    Ok(())
}

async fn archive_issue(id: &str, output: &OutputOptions, agent_opts: AgentOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let issue_id = resolve_issue_id(&client, id, true).await?;
    let dry_run = output.dry_run || agent_opts.dry_run;

    if dry_run {
        if output.is_json() || output.has_template() {
            print_json(
                &json!({
                    "dry_run": true,
                    "would_archive": true,
                    "id": issue_id,
                }),
                output,
            )?;
        } else {
            println!("{}", "[DRY RUN] Would archive issue:".yellow().bold());
            println!("  ID: {}", id);
        }
        return Ok(());
    }

    let mutation = r#"
        mutation($id: String!) {
            issueArchive(id: $id) {
                success
                entity {
                    id
                    identifier
                    title
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": issue_id })))
        .await?;

    if result["data"]["issueArchive"]["success"].as_bool() == Some(true) {
        let issue = &result["data"]["issueArchive"]["entity"];
        let identifier = issue["identifier"].as_str().unwrap_or(id);

        if agent_opts.id_only {
            println!("{}", identifier);
            return Ok(());
        }

        if output.is_json() || output.has_template() {
            print_json(issue, output)?;
            return Ok(());
        }

        if agent_opts.quiet {
            println!("{}", identifier);
            return Ok(());
        }

        println!(
            "{} Archived issue: {} {}",
            "+".green(),
            identifier,
            issue["title"].as_str().unwrap_or("")
        );
    } else {
        anyhow::bail!("Failed to archive issue");
    }

    Ok(())
}

async fn unarchive_issue(id: &str, output: &OutputOptions, agent_opts: AgentOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let issue_id = resolve_issue_id(&client, id, true).await?;
    let dry_run = output.dry_run || agent_opts.dry_run;

    if dry_run {
        if output.is_json() || output.has_template() {
            print_json(
                &json!({
                    "dry_run": true,
                    "would_archive": false,
                    "id": issue_id,
                }),
                output,
            )?;
        } else {
            println!("{}", "[DRY RUN] Would unarchive issue:".yellow().bold());
            println!("  ID: {}", id);
        }
        return Ok(());
    }

    let mutation = r#"
        mutation($id: String!) {
            issueUnarchive(id: $id) {
                success
                entity {
                    id
                    identifier
                    title
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": issue_id })))
        .await?;

    if result["data"]["issueUnarchive"]["success"].as_bool() == Some(true) {
        let issue = &result["data"]["issueUnarchive"]["entity"];
        let identifier = issue["identifier"].as_str().unwrap_or(id);

        if agent_opts.id_only {
            println!("{}", identifier);
            return Ok(());
        }

        if output.is_json() || output.has_template() {
            print_json(issue, output)?;
            return Ok(());
        }

        if agent_opts.quiet {
            println!("{}", identifier);
            return Ok(());
        }

        println!(
            "{} Unarchived issue: {} {}",
            "+".green(),
            identifier,
            issue["title"].as_str().unwrap_or("")
        );
    } else {
        anyhow::bail!("Failed to unarchive issue");
    }

    Ok(())
}

async fn remind_issue(
    id: &str,
    at: Option<String>,
    delay: Option<String>,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;
    let issue_id = resolve_issue_id(&client, id, true).await?;

    let reminder_at = match (at, delay) {
        (Some(at), None) => at,
        (None, Some(delay)) => {
            let delta = parse_reminder_delay(&delay)?;
            let when = Local::now() + delta;
            when.with_timezone(&Utc).to_rfc3339()
        }
        _ => {
            anyhow::bail!("Provide either --at or --in for reminders.");
        }
    };

    let mutation = r#"
        mutation($id: String!, $reminderAt: DateTime!) {
            issueReminder(id: $id, reminderAt: $reminderAt) {
                success
                issue {
                    id
                    identifier
                    title
                }
            }
        }
    "#;

    let result = client
        .mutate(
            mutation,
            Some(json!({ "id": issue_id, "reminderAt": reminder_at })),
        )
        .await?;

    if result["data"]["issueReminder"]["success"].as_bool() == Some(true) {
        if output.is_json() || output.has_template() {
            let response = json!({
                "success": true,
                "issueId": issue_id,
                "issue": id,
                "reminderAt": reminder_at,
            });
            print_json(&response, output)?;
            return Ok(());
        }
        println!("{} Reminder set for {}", "+".green(), id);
    } else {
        anyhow::bail!("Failed to set reminder");
    }

    Ok(())
}

async fn subscribe_issue(id: &str, user: Option<String>, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let issue_id = resolve_issue_id(&client, id, true).await?;
    let user_value = user.unwrap_or_else(|| "me".to_string());
    let user_id = resolve_user_id(&client, &user_value).await?;

    let mutation = r#"
        mutation($id: String!, $userId: String!) {
            issueSubscribe(id: $id, userId: $userId) {
                success
                issue {
                    id
                    identifier
                    title
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": issue_id, "userId": user_id })))
        .await?;

    if result["data"]["issueSubscribe"]["success"].as_bool() == Some(true) {
        let issue = &result["data"]["issueSubscribe"]["issue"];
        if output.is_json() || output.has_template() {
            print_json(issue, output)?;
            return Ok(());
        }
        println!(
            "{} Subscribed to {}",
            "+".green(),
            issue["identifier"].as_str().unwrap_or(id)
        );
    } else {
        anyhow::bail!("Failed to subscribe to issue");
    }

    Ok(())
}

async fn unsubscribe_issue(id: &str, user: Option<String>, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let issue_id = resolve_issue_id(&client, id, true).await?;
    let user_value = user.unwrap_or_else(|| "me".to_string());
    let user_id = resolve_user_id(&client, &user_value).await?;

    let mutation = r#"
        mutation($id: String!, $userId: String!) {
            issueUnsubscribe(id: $id, userId: $userId) {
                success
                issue {
                    id
                    identifier
                    title
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": issue_id, "userId": user_id })))
        .await?;

    if result["data"]["issueUnsubscribe"]["success"].as_bool() == Some(true) {
        let issue = &result["data"]["issueUnsubscribe"]["issue"];
        if output.is_json() || output.has_template() {
            print_json(issue, output)?;
            return Ok(());
        }
        println!(
            "{} Unsubscribed from {}",
            "+".green(),
            issue["identifier"].as_str().unwrap_or(id)
        );
    } else {
        anyhow::bail!("Failed to unsubscribe from issue");
    }

    Ok(())
}

async fn get_issue_team_id(client: &LinearClient, issue_id: &str) -> Result<String> {
    let query = r#"
        query($id: String!) {
            issue(id: $id) {
                id
                team { id }
            }
        }
    "#;

    let result = client.query(query, Some(json!({ "id": issue_id }))).await?;
    let team_id = result["data"]["issue"]["team"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Could not resolve team for issue"))?;
    Ok(team_id.to_string())
}

fn read_json_data(data: Option<&str>) -> Result<Option<Value>> {
    let Some(data) = data else { return Ok(None) };
    let raw = if data == "-" {
        let stdin = io::stdin();
        let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();
        lines.join("\n")
    } else {
        data.to_string()
    };
    let value: Value = serde_json::from_str(&raw)?;
    Ok(Some(value))
}

async fn delete_issue(id: &str, force: bool, agent_opts: AgentOptions) -> Result<()> {
    if !force && !agent_opts.quiet {
        let confirm = dialoguer::Confirm::new()
            .with_prompt(format!("Delete issue {}? This cannot be undone", id))
            .default(false)
            .interact()?;

        if !confirm {
            println!("Cancelled.");
            return Ok(());
        }
    } else if !force && agent_opts.quiet {
        // In quiet mode without force, require --force
        anyhow::bail!("Use --force to delete in quiet mode");
    }

    let client = LinearClient::new()?;

    let mutation = r#"
        mutation($id: String!) {
            issueDelete(id: $id) {
                success
            }
        }
    "#;

    let result = client.mutate(mutation, Some(json!({ "id": id }))).await?;

    if result["data"]["issueDelete"]["success"].as_bool() == Some(true) {
        if !agent_opts.quiet {
            println!("{} Issue deleted", "+".green());
        }
    } else {
        anyhow::bail!("Failed to delete issue");
    }

    Ok(())
}

// Git helper functions for start command
fn run_git_command(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Git command failed: {}", stderr.trim());
    }
}

fn branch_exists(branch: &str) -> bool {
    run_git_command(&["rev-parse", "--verify", branch]).is_ok()
}

fn generate_branch_name(identifier: &str, title: &str) -> String {
    // Convert title to kebab-case for branch name
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    // Truncate if too long
    let slug = if slug.len() > 50 {
        slug[..50].trim_end_matches('-').to_string()
    } else {
        slug
    };

    format!("{}/{}", identifier.to_lowercase(), slug)
}

async fn start_issue(
    id: &str,
    checkout: bool,
    custom_branch: Option<String>,
    agent_opts: AgentOptions,
) -> Result<()> {
    let client = LinearClient::new()?;

    // First, get the issue details including team info to find the "started" state
    let query = r#"
        query($id: String!) {
            issue(id: $id) {
                id
                identifier
                title
                branchName
                team {
                    id
                    states {
                        nodes {
                            id
                            name
                            type
                        }
                    }
                }
            }
            viewer {
                id
            }
        }
    "#;

    let result = client.query(query, Some(json!({ "id": id }))).await?;
    let issue = &result["data"]["issue"];

    if issue.is_null() {
        anyhow::bail!("Issue not found: {}", id);
    }

    let identifier = issue["identifier"].as_str().unwrap_or("");
    let title = issue["title"].as_str().unwrap_or("");
    let linear_branch = issue["branchName"].as_str().unwrap_or("").to_string();

    // Get current user ID
    let viewer_id = result["data"]["viewer"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Could not fetch current user ID"))?;

    // Find a "started" type state (In Progress)
    let empty = vec![];
    let states = issue["team"]["states"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    let started_state = states
        .iter()
        .find(|s| s["type"].as_str() == Some("started"));

    let state_id = match started_state {
        Some(s) => s["id"].as_str().unwrap_or(""),
        None => anyhow::bail!("No 'started' state found for this team"),
    };

    let state_name = started_state
        .and_then(|s| s["name"].as_str())
        .unwrap_or("In Progress");

    // Update the issue: set state to "In Progress" and assign to current user
    let input = json!({
        "stateId": state_id,
        "assigneeId": viewer_id
    });

    let mutation = r#"
        mutation($id: String!, $input: IssueUpdateInput!) {
            issueUpdate(id: $id, input: $input) {
                success
                issue {
                    identifier
                    title
                    state { name }
                    assignee { name }
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": id, "input": input })))
        .await?;

    if result["data"]["issueUpdate"]["success"].as_bool() == Some(true) {
        let updated = &result["data"]["issueUpdate"]["issue"];
        let updated_id = updated["identifier"].as_str().unwrap_or("");

        if agent_opts.id_only {
            println!("{}", updated_id);
        } else if !agent_opts.quiet {
            println!(
                "{} Started issue: {} {}",
                "+".green(),
                updated_id.cyan(),
                updated["title"].as_str().unwrap_or("")
            );
            println!(
                "  State:    {}",
                updated["state"]["name"].as_str().unwrap_or(state_name)
            );
            println!(
                "  Assignee: {}",
                updated["assignee"]["name"].as_str().unwrap_or("me")
            );
        }
    } else {
        anyhow::bail!("Failed to start issue");
    }

    // Optionally checkout a git branch
    if checkout {
        let branch_name = custom_branch
            .or(if linear_branch.is_empty() {
                None
            } else {
                Some(linear_branch)
            })
            .unwrap_or_else(|| generate_branch_name(identifier, title));

        if !agent_opts.quiet {
            println!();
        }
        if branch_exists(&branch_name) {
            if !agent_opts.quiet {
                println!("Checking out existing branch: {}", branch_name.green());
            }
            run_git_command(&["checkout", &branch_name])?;
        } else {
            if !agent_opts.quiet {
                println!("Creating and checking out branch: {}", branch_name.green());
            }
            run_git_command(&["checkout", "-b", &branch_name])?;
        }

        let current = run_git_command(&["rev-parse", "--abbrev-ref", "HEAD"])?;
        if !agent_opts.quiet {
            println!("{} Now on branch: {}", "+".green(), current);
        }
    }

    Ok(())
}

async fn stop_issue(id: &str, unassign: bool, agent_opts: AgentOptions) -> Result<()> {
    let client = LinearClient::new()?;

    // First, get the issue details including team info to find the "backlog" or "unstarted" state
    let query = r#"
        query($id: String!) {
            issue(id: $id) {
                id
                identifier
                title
                team {
                    id
                    states {
                        nodes {
                            id
                            name
                            type
                        }
                    }
                }
            }
        }
    "#;

    let result = client.query(query, Some(json!({ "id": id }))).await?;
    let issue = &result["data"]["issue"];

    if issue.is_null() {
        anyhow::bail!("Issue not found: {}", id);
    }

    // Find a "backlog" or "unstarted" type state
    let empty = vec![];
    let states = issue["team"]["states"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    // Prefer backlog, fall back to unstarted
    let stop_state = states
        .iter()
        .find(|s| s["type"].as_str() == Some("backlog"))
        .or_else(|| {
            states
                .iter()
                .find(|s| s["type"].as_str() == Some("unstarted"))
        });

    let state_id = match stop_state {
        Some(s) => s["id"].as_str().unwrap_or(""),
        None => anyhow::bail!("No 'backlog' or 'unstarted' state found for this team"),
    };

    let state_name = stop_state
        .and_then(|s| s["name"].as_str())
        .unwrap_or("Backlog");

    // Build the update input
    let mut input = json!({
        "stateId": state_id
    });

    // Optionally unassign
    if unassign {
        input["assigneeId"] = json!(null);
    }

    let mutation = r#"
        mutation($id: String!, $input: IssueUpdateInput!) {
            issueUpdate(id: $id, input: $input) {
                success
                issue {
                    identifier
                    title
                    state { name }
                    assignee { name }
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": id, "input": input })))
        .await?;

    if result["data"]["issueUpdate"]["success"].as_bool() == Some(true) {
        let updated = &result["data"]["issueUpdate"]["issue"];
        let updated_id = updated["identifier"].as_str().unwrap_or("");

        if agent_opts.id_only {
            println!("{}", updated_id);
        } else if !agent_opts.quiet {
            println!(
                "{} Stopped issue: {} {}",
                "+".green(),
                updated_id.cyan(),
                updated["title"].as_str().unwrap_or("")
            );
            println!(
                "  State:    {}",
                updated["state"]["name"].as_str().unwrap_or(state_name)
            );
            if unassign {
                println!("  Assignee: (unassigned)");
            } else if let Some(assignee) = updated["assignee"]["name"].as_str() {
                println!("  Assignee: {}", assignee);
            }
        }
    } else {
        anyhow::bail!("Failed to stop issue");
    }

    Ok(())
}
