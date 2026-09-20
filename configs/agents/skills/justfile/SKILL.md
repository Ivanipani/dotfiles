---
name: justfile
description: Create or modify justfiles with discoverable recipes, parameters, error handling, and optional fzf selection.
---

# Justfile Skill

Inspect the existing justfile and recipe conventions first. Keep explicit recipe parameters usable without fzf or a TTY; offer fuzzy selection as an interactive convenience. Inspect recipes before running them and use `just --list` to check discoverability. Run only recipes relevant to the requested task.

## Philosophy

1. **Discoverability**: Users should easily find and understand available commands
2. **Fuzzy Finding**: Interactive selection for complex inputs using fzf
3. **Error Prevention**: Validate inputs and provide helpful error messages
4. **Self-Documenting**: Every recipe should explain itself

## Documentation

- Just: https://github.com/casey/just
- fzf: https://github.com/junegunn/fzf

## Basic Syntax

```just
# Recipe with documentation comment (shown in --list)
recipe-name:
    command

# Recipe with parameters
greet name:
    echo "Hello, {{name}}!"

# Recipe with default parameter
test count="5":
    pytest -n {{count}}

# Recipe with dependencies
build: lint test
    cargo build --release
```

## Documentation Best Practices

### Use Documentation Comments

Comments immediately before a recipe appear in `just --list`:

```just
# Run the test suite with coverage reporting
test:
    pytest --cov

# Deploy to production (requires VPN connection)
deploy:
    ./scripts/deploy.sh
```

### Group Related Recipes

Use comment headers to organize recipes:

```just
# ============================================================================
# Development
# ============================================================================

# Start development server with hot reload
dev:
    npm run dev

# Run linter and formatter
lint:
    npm run lint

# ============================================================================
# Testing
# ============================================================================

# Run full test suite
test:
    npm test
```

### Default Recipe

Always define a helpful default that shows available commands:

```just
# Show available recipes
[private]
default:
    @just --list --unsorted
```

Or with fzf for interactive selection:

```just
# Interactively select and run a recipe
[private]
default:
    @just --list --unsorted | tail -n +2 | fzf --height 40% --reverse | awk '{print $1}' | xargs -r just
```

## Detailed reference

Consult [references/guide.md](references/guide.md) for the following topics. Search for the relevant heading and read that section as needed:

- fzf Integration Patterns
- Error Handling Patterns
- Parameter Patterns
- Shebang Recipes
- Complete Example Justfile
- Just Settings
- Tips
