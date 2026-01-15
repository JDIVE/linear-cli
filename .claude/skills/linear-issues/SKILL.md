---
name: linear-issues
description: Manage Linear issues - list, create, update, start/stop work. Use when working with Linear issues, viewing tasks, creating bugs, or updating issue status.
allowed-tools: Bash
---

# Linear Issues

Manage Linear.app issues using the `linear-cli` command-line tool.

## List Issues

```bash
# List all issues
linear-cli i list

# Filter by team
linear-cli i list -t Engineering

# Filter by status
linear-cli i list -s "In Progress"

# Get JSON output (for parsing)
linear-cli i list --output json
```

## View Issue Details

```bash
# View issue details
linear-cli i get LIN-123

# Get as JSON
linear-cli i get LIN-123 --output json

# Batch fetch multiple issues
linear-cli i get LIN-1 LIN-2 LIN-3 --output json
```

## Create Issues

```bash
# Create issue (priority: 1=urgent, 2=high, 3=medium, 4=low)
linear-cli i create "Bug: Login fails" -t Engineering -p 2

# Create with status
linear-cli i create "Feature request" -t ENG -s "Backlog"

# Preview without creating (dry run)
linear-cli i create "Test issue" -t ENG --dry-run

# Get just the created ID (for chaining)
linear-cli i create "Bug fix" -t ENG --id-only

# Read description from stdin
cat description.md | linear-cli i create "Title" -t ENG -d -
```

## Update Issues

```bash
# Update status
linear-cli i update LIN-123 -s Done

# Update priority
linear-cli i update LIN-123 -p 1

# Get just the ID on success
linear-cli i update LIN-123 -s Done --id-only
```

## Start/Stop Work

```bash
# Start working (assigns to you, sets In Progress, creates git branch)
linear-cli i start LIN-123 --checkout

# Stop working (unassigns, resets status)
linear-cli i stop LIN-123
```

## Comments

```bash
# List comments
linear-cli cm list LIN-123

# Get comments as JSON
linear-cli cm list LIN-123 --output json

# Add comment
linear-cli cm create LIN-123 -b "Fixed in latest commit"
```

## Context Detection

```bash
# Get current issue from git branch name
linear-cli context

# Get with full issue details as JSON
linear-cli context --output json
```

## Agent-Friendly Options

| Flag | Purpose |
|------|---------|
| `--output json` | Machine-readable JSON output |
| `--quiet` / `-q` | Suppress decorative output |
| `--id-only` | Only output created/updated ID |
| `--dry-run` | Preview without executing |

## Exit Codes
- `0` = Success
- `1` = General error
- `2` = Not found
- `3` = Auth error

## Tips

- Use `--output json` for machine-readable output
- Short alias: `i` for issues, `cm` for comments, `ctx` for context
- Run `linear-cli i --help` for all options
