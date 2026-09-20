---
name: jujutsu
description: Inspect and manage Jujutsu (jj) repositories, including revsets, changes, conflicts, bookmarks, workspaces, and history.
---

# Jujutsu (jj) Version Control Skill

Inspect `jj status` and `jj diff` before changing history. Use explicit revisions and file paths to preserve unrelated work. Prefer non-interactive commands and supplied descriptions; use editors, merge tools, or interactive selection only when an interactive terminal is available. Commit or publish only within the user's requested scope. Check `jj help <command>` for version-specific flags.

Use Conventional Commits for descriptions; see the [message specification](../conventional-commits/SKILL.md) when composing one.

## Core Concepts

### What is Jujutsu?
Jujutsu is an experimental VCS that:
- Tracks changes automatically (automatic snapshotting)
- Uses a change-centric model (changes have IDs independent of commits)
- Supports conflict-first workflows
- Has powerful revset expressions for selecting commits
- Maintains an operation log for undo/redo at any time
- Can work as a Git-compatible frontend

### Key Differences from Git
- **Working copy commit (`@`)**: Your current work is always a commit
- **Change IDs**: Changes are tracked independently of commits
- **Operation log**: Every command creates an operation that can be undone
- **Automatic snapshots**: Working copy is snapshotted before every command
- **Conflict tracking**: Conflicts are first-class citizens stored in commits
- **No staging area**: Changes are committed directly

## Best Practices for Coding Agents

When helping users with jj:

1. **Always check if in jj repo**: Run `jj status` to verify
2. **Use revsets for precision**: Prefer specific revsets over vague references
3. **Use the operation log for recovery**: Inspect the relevant operation before undoing it; remote side effects are not undone locally
4. **Explain change IDs vs commit IDs**: Clarify when discussing revisions
5. **Use `--at-op` for investigation**: Show repo state at different times
6. **Check for conflicts**: Run `jj status` or `jj log -r 'conflicts()'`
7. **Handle divergence carefully**: Identify and resolve divergent changes properly
8. **Inspect mutation results**: Check each history-changing command before choosing the next operation
9. **Test revsets**: Use `jj log -r <revset>` to verify before destructive operations
10. **Preserve user intent**: Use `present()` and `coalesce()` for robust scripts
11. **Always use Conventional Commits**: Every `jj describe` or `jj commit -m` MUST use the `type[(scope)][!]: description` format. Suggest the correct type when helping users write descriptions.

## Detailed reference

Consult [references/guide.md](references/guide.md) for the following topics. Search for the relevant heading and read that section as needed:

- Revsets: Selecting Commits
- Change History and Log
- Operation Log
- Working with Workspaces
- Fixing Divergent Changes
- Conflict Resolution
- Change Management
- Bookmarks and Tags
- Git Integration
- Common Workflows
- Reference: All Commands
- Getting Help
