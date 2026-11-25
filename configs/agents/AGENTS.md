## Guidelines for coding agents

- NEVER use git commands. Prefer jujutsu for viewing commit history.
- Avoid over-explaining technical decisions. Your comments should be brief and direct.
- Follow the Don't Repeat Yourself principle when writing code. Avoid duplicating existing code.
- Write all tests in a GIVEN/WHEN/THEN fashion. A test should aim to exercise at most one condition. The test name should clearly describe
what is being tested and what the expectation is such that if the test fails, we know exactly why just from the name

## Model delegation (Claude Code)

I explicitly authorize and expect subagent use for the task types below; do not wait to be asked. Keep the main thread for idea generation, planning, bug-finding, and review.

Granularity: delegate a whole unit of work, i.e. babysitting a process (deploy, CI, rollout, long test run) or implementing one part of a feature. Run one-off quick commands inline.

- `implementer` (sonnet): implementing a part of a feature from an agreed plan, with its tests.
- `ops-runner` (haiku): babysitting a process, plus merging, triggering deploys, and reading logs.
- `homelab-debugger` (opus): read-only diagnosis of unhealthy/failing homelab components (k3s, Flux, Argo, Tekton, DNS/TLS). Proposes fixes, never applies them.
- `toolsmith` (sonnet): how to get more out of Docker, Pants, Kubernetes, Argo, Ansible, and custom CLIs like reek; finds underused features and gaps.
- `simplifier` (opus): read-only review for tech debt, ill-fitting abstractions, and needless complexity; proposes simpler designs.
- Searching/reading code: `Explore` (haiku) or `model: "haiku"`.
- Always set the model explicitly. Never use opus/fable for a subagent unless the task is design, root-causing a hard bug, or review.
- Subagents start cold: give them the plan, paths, and success condition, and ask for a brief report.
