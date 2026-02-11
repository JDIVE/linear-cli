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
```

## Create Issues

```bash
# Create issue (priority: 1=urgent, 2=high, 3=normal, 4=low)
linear-cli i create "Bug: Login fails" -t Engineering -p 2

# Create with status
linear-cli i create "Feature request" -t ENG -s "Backlog"
```

## Update Issues

```bash
# Update status
linear-cli i update LIN-123 -s Done

# Update priority
linear-cli i update LIN-123 -p 1
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

## Issue Documents

```bash
# List documents linked to an issue
linear-cli i documents list LIN-123 --output json

# Create a document linked to an issue
linear-cli i documents create LIN-123 "Runbook"

# Attach an existing document to an issue
linear-cli i documents add LIN-123 DOC_ID

# Update an issue-linked document
linear-cli i documents update DOC_ID --title "Updated title"

# Remove document from issue (does not delete document)
linear-cli i documents remove DOC_ID
```

## Tips

- Use `--output json` for machine-readable output
- Short alias: `i` for issues, `cm` for comments, `d` for documents
- Prefer issue-scoped docs workflow: `i documents ...` (`d ... --issue ...` is still supported)
- Run `linear-cli i --help` for all options
