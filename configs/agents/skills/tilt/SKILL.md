---
name: tilt
description: Create, debug, or improve Tiltfiles for Kubernetes, Docker Compose, and hybrid local development, including live updates and resource dependencies.
---

# Tilt Skill

Inspect existing Tiltfiles, manifests, and the installed Tilt version first. Preserve resource names and deployment context. Loading a Tiltfile can execute local commands and start deployments; inspect those effects before running Tilt. Scope live changes to the environment and resources requested by the user.

## Documentation

- Tiltfile authoring: https://docs.tilt.dev/tiltfile_authoring.html
- API reference: https://docs.tilt.dev/api.html
- API server reference: https://api.tilt.dev/

## Philosophy

1. **Fast feedback loops**: Tilt exists to make inner dev loops as fast as possible — prefer live updates over full rebuilds
2. **Declarative**: Express what you want; Tilt figures out ordering and dependencies
3. **Composable**: Break complex Tiltfiles into helper functions and loaded files
4. **Developer-friendly**: Expose config options so devs can customize without editing the Tiltfile

## Tiltfile Basics

Tiltfiles are written in **Starlark**, a Python dialect. Every Tiltfile is re-executed when it or any watched file changes.

```python
# Minimal Kubernetes Tiltfile
docker_build("my-registry/my-app", ".")
k8s_yaml("k8s/deployment.yaml")
k8s_resource("my-app", port_forwards=8080)
```

```python
# Minimal Docker Compose Tiltfile
docker_compose("./docker-compose.yml")
```

## Detailed reference

Consult [references/guide.md](references/guide.md) for the following topics. Search for the relevant heading and read that section as needed:

- Docker Build
- Docker Compose
- Kubernetes Resources
- Helm
- Local Resources
- Live Update
- Configuration & Parameterization
- Utilities
- Common Patterns
- Trigger Modes
- Tips
