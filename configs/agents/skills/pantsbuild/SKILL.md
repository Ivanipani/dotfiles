---
name: pantsbuild
description: Configure Pants builds and BUILD targets, edit pants.toml, and run scoped goals for Python, Docker, Shell, and ad-hoc tools.
---

# Pantsbuild Skill

Read `pants.toml` and nearby BUILD files first. Preserve the pinned Pants version and enabled backends. Use the project's Pants launcher and scope goals to affected targets; check installed help for version-specific target fields. Review generated BUILD or lockfile changes before finishing.

## Documentation

- Welcome: https://www.pantsbuild.org/stable/docs/introduction/welcome-to-pants
- Targets & BUILD files: https://www.pantsbuild.org/stable/docs/using-pants/key-concepts/targets-and-build-files
- Python: https://www.pantsbuild.org/stable/docs/python/overview
- Docker: https://www.pantsbuild.org/stable/docs/docker
- Shell: https://www.pantsbuild.org/stable/docs/shell
- Ad-hoc tools: https://www.pantsbuild.org/stable/docs/ad-hoc-tools/ad-hoc-tool-overview

## Core Concepts

### Target Addressing

```
path/to/dir:name      # fully qualified
:name                 # relative to current BUILD file
//:name               # root of repo
path/to/dir           # shorthand when target name == directory name
path/to/file.py       # generated target from a generator (e.g. python_sources)
path/to:gen#dep       # generated third-party dep target
```

### BUILD File Builtins

```python
# Apply defaults across a directory subtree
__defaults__(
    {
        python_sources: {"resolve": "python-default"},
        python_tests: {"timeout": 60},
    }
)

# Parametrize a target (creates multiple variants)
shunit2_tests(
    name="tests",
    shell=parametrize("bash", "zsh"),
)

# Read environment variables
docker_image(
    name="app",
    extra_build_args=[f"VERSION={env('BUILD_VERSION', 'dev')}"],
)
```

### Dependency Syntax

```python
python_sources(
    name="lib",
    dependencies=[
        "other/dir:lib",  # first-party target
        "3rdparty:reqs#django",  # third-party requirement
        "!helloworld/other:lib",  # exclude inferred dep (single !)
    ],
)

pex_binary(
    name="app",
    dependencies=["!!some/transitive:dep"],  # transitive exclusion (leaf targets only)
)
```

## Detailed reference

Consult [references/guide.md](references/guide.md) for the following topics. Search for the relevant heading and read that section as needed:

- `pants.toml` Configuration
- Python Integration
- Docker Integration
- Shell Integration
- Ad-Hoc Tools Integration
- Universal CLI Reference
- Auto-generation with `tailor`
- Tips
