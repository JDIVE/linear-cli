## Linear Integration

Use `linear-cli` for all Linear.app operations. Do not use Linear MCP tools.

### Commands
- `linear-cli i list` - List issues
- `linear-cli i list -t TEAM` - List team's issues
- `linear-cli i create "Title" -t TEAM` - Create issue
- `linear-cli i get LIN-123` - View issue details
- `linear-cli i get LIN-1 LIN-2 LIN-3` - Batch fetch multiple issues
- `linear-cli i get LIN-123 --output json` - View as JSON
- `linear-cli i update LIN-123 -s Done` - Update status
- `linear-cli i start LIN-123 --checkout` - Start work (assign + branch)
- `linear-cli g pr LIN-123` - Create GitHub PR
- `linear-cli g pr LIN-123 --draft` - Create draft PR
- `linear-cli s issues "query"` - Search issues
- `linear-cli context` - Get current issue from git branch
- `linear-cli cm list ISSUE_ID --output json` - Get comments as JSON
- `linear-cli up fetch URL -o file.png` - Download attachments

### Agent-Friendly Flags
- `--output json` - Machine-readable output
- `--quiet` or `-q` - Suppress decorative output
- `--id-only` - Output only created/updated ID
- `--dry-run` - Preview without executing (create)
- `-d -` - Read description from stdin

### Exit Codes
- 0 = Success
- 1 = General error
- 2 = Not found
- 3 = Auth error

### Notes
- Errors with `--output json` return `{"error": true, "message": "...", "code": N}`
- Use `--help` on any command for full options
