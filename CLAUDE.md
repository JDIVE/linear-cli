## Linear Integration

Use `linear-cli` for all Linear.app operations. Do NOT use Linear MCP tools - CLI is 10-50x more token-efficient.

### Quick Commands

| Task | Command |
|------|---------|
| List issues | `linear-cli i list` |
| Create issue | `linear-cli i create "Title" -t TEAM -p 2` |
| View issue | `linear-cli i get LIN-123` |
| Get multiple | `linear-cli i get LIN-1 LIN-2 LIN-3` |
| Start work | `linear-cli i start LIN-123 --checkout` |
| Update status | `linear-cli i update LIN-123 -s Done` |
| Create PR | `linear-cli g pr LIN-123` |
| Search | `linear-cli s issues "query"` |
| Get context | `linear-cli context` |
| Get comments | `linear-cli cm list ISSUE_ID --output json` |
| Download upload | `linear-cli up fetch URL -o file.png` |

### Agent-Friendly Options

| Flag | Purpose |
|------|---------|
| `--output json` | Machine-readable JSON output |
| `--quiet` | Suppress decorative output |
| `--id-only` | Only output created/updated ID |
| `--dry-run` | Preview without executing (create) |
| `-` (stdin) | Read description/IDs from pipe |

### Examples for Agents

```bash
# Get current issue from branch
linear-cli context --output json

# Create and get ID for chaining
linear-cli i create "Bug" -t ENG --id-only

# Quiet create, capture ID
ID=$(linear-cli i create "Task" -t ENG -q --id-only)

# Preview without creating
linear-cli i create "Test" -t ENG --dry-run

# Batch fetch multiple issues
linear-cli i get LIN-1 LIN-2 LIN-3 --output json

# Pipe description from file
cat desc.md | linear-cli i create "Title" -t ENG -d -

# Structured error handling
linear-cli i get INVALID --output json  # Returns {"error": true, ...}
```

### Exit Codes
- `0` = Success
- `1` = General error
- `2` = Not found
- `3` = Auth error

### Tips
- Use short aliases: `i` (issues), `p` (projects), `g` (git), `s` (search), `cm` (comments), `ctx` (context)
- Run `linear-cli <command> --help` for full options
