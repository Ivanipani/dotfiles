## Guidelines for coding agents

- NEVER use git commands. Prefer jujutsu for viewing commit history.
- Avoid over-explaining technical decisions. Your comments should be brief and direct.
- Follow the Don't Repeat Yourself principle when writing code. Avoid duplicating existing code.
- Write all tests in a GIVEN/WHEN/THEN fashion. A test should aim to exercise at most one condition. The test name should clearly describe
what is being tested and what the expectation is such that if the test fails, we know exactly why just from the name
