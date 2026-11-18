## Guidelines for coding agents

- NEVER use git commands. Prefer jujutsu for viewing commit history.
- Avoid over-explaining technical decisions. Your comments should be brief and direct.
- Follow the Don't Repeat Yourself principle when writing code. Avoid duplicating existing code.
- Write all tests in a GIVEN/WHEN/THEN fashion. A test should aim to exercise at most one condition. The test name should clearly describe
what is being tested and what the expectation is such that if the test fails, we know exactly why just from the name

## Testing CLI/PTY changes in Rex

When testing CLI, TUI, or PTY behavior, use a Rex session so the operator can watch in realtime. Load the `rex` skill first.

- Create a dedicated test session, never reuse the operator's or your own: `rex new <task>-test -c "$PWD" --json`. Name it for the work (two to four lowercase hyphenated words, e.g. `fix-resize-test`). Save the returned IDs.
- Organize the session: one tab per scenario or concern (`rex window new -s SESSION <name>`), and panes within a tab for related processes (app under test, logs, a driver shell). Label panes (`--label`).
- Drive the program with `rex send` / `rex send-key` and verify with `rex capture -s SESSION -b BLOCK --trim`. Use `rex block call ... process` for exit status and `modes` to detect full-screen apps. Prefer this over piping output or mocking a PTY.
- Pass `--keep-open` for anything the operator should read after it exits. Use `--focus=false` for background work.
- Rebuild before each run so you test current code. Reproduce the bug first, then confirm the fix in the same session.
- Leave the session open when done so the operator can inspect it, and report its name. Only `rex kill` sessions you created, and only when asked or after the operator has seen the result.
- Never send input to your own pane or touch sessions you did not create.
