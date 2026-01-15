---
name: linear-pr
description: Create GitHub PRs linked to Linear issues. Use when creating pull requests, pushing code for review, or linking PRs to Linear issues.
allowed-tools: Bash
---

# Linear PR Creation

Create GitHub pull requests linked to Linear issues using `linear-cli`.

## Create PR for Issue

```bash
# Create PR linked to Linear issue
linear-cli g pr LIN-123

# Create draft PR
linear-cli g pr LIN-123 --draft

# Specify base branch
linear-cli g pr LIN-123 --base main

# Open in browser after creation
linear-cli g pr LIN-123 --web
```

## Git Branch Operations

```bash
# Create and checkout branch for issue
linear-cli g checkout LIN-123

# Use custom branch name
linear-cli g checkout LIN-123 -b my-custom-branch

# Just show branch name (don't create)
linear-cli g branch LIN-123

# Create branch without checkout
linear-cli g create LIN-123
```

## Full Workflow

```bash
# 1. Start working on issue (assigns, sets In Progress, creates branch)
linear-cli i start LIN-123 --checkout

# 2. Make your changes...
# git add . && git commit -m "Fix the bug"

# 3. Create PR
linear-cli g pr LIN-123
```

## Tips

- PR title and description are auto-generated from issue
- Use `--draft` for work-in-progress PRs
- The branch name follows pattern: `username/lin-123-issue-title`
