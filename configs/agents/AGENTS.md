## Guidelines for coding agents

- NEVER use git commands. Prefer jujutsu for viewing commit history.
- Avoid over-explaining technical decisions. Your comments should be brief and direct.
- Follow the Don't Repeat Yourself principle when writing code. Avoid duplicating existing code.
- Write all tests in a GIVEN/WHEN/THEN fashion. A test should aim to exercise at most one condition. The test name should clearly describe
what is being tested and what the expectation is such that if the test fails, we know exactly why just from the name

## Model delegation (Claude Code)

Keep the main thread for idea generation, planning, bug-finding, and review. Delegate the rest via the Agent tool, always setting the model explicitly:

- `implementer` (sonnet): writing code and tests from an agreed plan.
- `ops-runner` (haiku): running commands/tests, merging, triggering and waiting on deploys or CI, polling status, reading logs.
- Searching/reading code: `Explore` (haiku) or `model: "haiku"`.
- Never use opus/fable for a subagent unless the task is design, root-causing a hard bug, or review.
- Give subagents full context (plan, paths, success condition) and ask for a brief report.
