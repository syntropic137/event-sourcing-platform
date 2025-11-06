# Claude Code Slash Commands

This directory contains custom slash commands for Claude Code to streamline development workflows.

## Available Commands

### `/create-adr`

**Purpose:** Create a new Architecture Decision Record (ADR)

**Usage:**
```
/create-adr
```

**What it does:**
1. Scans `/docs/adrs/` to determine the next ADR number
2. Prompts for ADR title, problem statement, and related ADRs
3. Creates ADR files in **both** locations:
   - `/docs/adrs/ADR-XXX-{slug}.md`
   - `/docs-site/docs/adrs/ADR-XXX-{slug}.md`
4. Updates the ADR index at `/docs-site/docs/adrs/index.md`
5. Provides link to view in docs: `http://localhost:3001/adrs/ADR-XXX-{slug}`

**Template Structure:**
- Context and problem statement
- Decision and rationale
- Benefits and trade-offs
- Implementation details
- Consequences (positive, negative, neutral)
- Alternatives considered
- Migration path
- Validation metrics
- References
- Review history and approval

**Example:**
```
User: /create-adr

Claude: I'll help you create a new ADR. Let me scan existing ADRs...

Found: ADR-004 is the latest
Next ADR will be: ADR-005

What is the title of this ADR?
> Use PostgreSQL for Event Store

[... continues with prompts and creates files ...]
```

## Creating New Commands

To add a new slash command:

1. Create a JSON file in this directory: `{command-name}.json`
2. Structure:
```json
{
  "name": "command-name",
  "description": "Brief description shown in command palette",
  "prompt": "Detailed instructions for Claude..."
}
```

3. The `prompt` field should contain:
   - Clear task description
   - Step-by-step instructions
   - Template or format to follow
   - Reference files to check
   - Guidelines and best practices

## Best Practices

✅ **Use descriptive names** - `create-adr` not just `adr`  
✅ **Provide complete context** - Include file paths, formats, examples  
✅ **Reference existing work** - Point to similar files as templates  
✅ **Include validation** - Specify what to check/verify  
✅ **Document clearly** - Add entry to this README  

## References

- [Claude Code Documentation](https://docs.cursor.com/)
- [Project AGENTS.md](../../AGENTS.md) - RIPER-5 workflow modes
- [ADR Index](../../docs-site/docs/adrs/index.md) - View all ADRs

---

*These commands are part of the Event Sourcing Platform development workflow.*

