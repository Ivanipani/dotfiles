---
name: taskfile
description: Create, review, debug, or run Taskfiles for the go-task Task runner (taskfile.dev), including Taskfile.yml, task dependencies, includes, variables, and incremental builds. Not for Taskwarrior or generic task planning.
---

# Taskfile

## Workflow

1. Inspect the existing Taskfile, included files, project scripts, and CI usage. Preserve task names and the project's chosen tools.
2. Verify the executable is go-task: `task` can also mean Taskwarrior. Inspect its path and version before invoking tasks; use the project's configured binary or wrapper. Check installed help before using newer features.
3. Make the smallest change that supports the requested workflow. Reuse existing scripts for complex logic and Task tasks for orchestration.
4. Inspect command effects before validation or execution. Editing a deployment task does not itself request a deployment. Keep tasks callable with explicit inputs in non-interactive sessions.
5. Validate affected tasks and report what ran, what was skipped, and any missing tools.

## Task structure and ordering

- Use `version: '3'` for new files; preserve an existing project's version requirements.
- Give public tasks a useful `desc`; use `internal: true` for helpers.
- `deps` run concurrently before the parent commands. Use them only for independent prerequisites.
- Calls under `cmds` run sequentially. Use `- task: name` when order matters; avoid recursive shell invocations of Task for ordinary composition.
- Set `dir` deliberately. Finding a parent Taskfile changes the effective working directory; an included file's location does not automatically become its execution directory.
- Share tasks through namespaced `includes`; set an include's `dir` when needed. Mark an include optional only when its absence is acceptable.

See the [guide](https://taskfile.dev/docs/guide) for include resolution and task composition.

## Inputs and shell commands

- `vars` are Go-template values (`{{.NAME}}`); `env` supplies subprocess environment variables. Pass explicit values with `task greet NAME=value`.
- Quote YAML strings containing template syntax. Use a block scalar for shell commands containing YAML-sensitive punctuation such as `: `.
- Prefer environment variables and quoted shell expansions for data. If templating a value directly into a shell argument, use `{{.NAME | shellQuote}}` without adding another layer of shell quotes.
- Arguments after `--` are available as `.CLI_ARGS`. Forward them intentionally; do not treat a command argument list as one quoted scalar.
- Avoid shell-derived variables (`sh:`) when a static value suffices. They execute commands during value resolution.

See the [templating reference](https://taskfile.dev/docs/reference/templating) for functions and variable behavior.

## Incremental execution and failures

- Use `sources` for inputs and `generates` for outputs. Include configuration and lockfiles that affect the result; avoid broad globs that include generated output.
- `method: checksum` is the default change detector; choose `timestamp` only when modification times are appropriate.
- `status` checks whether work is already complete; successful checks skip execution. `preconditions` enforce prerequisites and fail when unmet. Do not interchange them.
- Use `requires.vars` for required inputs. Supply them explicitly in automation rather than relying on prompts.
- Preserve nonzero exit codes. Use `ignore_error` only when that specific failure is intentionally acceptable.

Consult the [schema reference](https://taskfile.dev/docs/reference/schema) for exact field types and supported options.

## Minimal example

This demonstrates explicit input, environment-based quoting, and ordered task calls. Replace the greeting tasks with project commands as needed.

```yaml
version: '3'

tasks:
  default:
    desc: List available tasks
    cmds:
      - task --list

  greet:
    desc: Greet a named recipient
    requires:
      vars: [NAME]
    env:
      RECIPIENT: '{{.NAME}}'
    cmds:
      - printf '%s\n' "$RECIPIENT"

  demo:
    desc: Run two greetings in order
    cmds:
      - task: greet
        vars: {NAME: first}
      - task: greet
        vars: {NAME: second}
```

## Validation

After confirming the executable and inspecting the Taskfile, use the installed equivalents of:

```sh
task --list                    # public tasks with descriptions
task --list-all                # also show undescribed public tasks
task --summary greet           # inspect the task summary
task --dry greet NAME=example  # inspect expanded commands
task greet NAME=example        # execute a harmless affected task
```

Use `--taskfile path/to/Taskfile.yml` to select a file explicitly. Dry runs preview commands; they are not an isolation boundary for dynamic variable resolution or remote includes. Inspect those before loading unfamiliar files. Avoid `--yes` or `--force` as blanket fixes for prompts or skipped work.

Check the [CLI reference](https://taskfile.dev/docs/reference/cli) for flags. When go-task is unavailable, validate YAML structure and explain that runtime behavior remains unverified.
