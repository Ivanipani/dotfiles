---
name: keycloak
description: Configure and troubleshoot Keycloak servers, realms, containers, Kubernetes deployments, databases, TLS, and reverse proxies.
---

# Keycloak Server Configuration & Management Skill

Identify the configured Keycloak version and deployment target before choosing options. Treat reference versions and credentials as examples; preserve version pins and use documentation matching the deployment. Editing manifests does not itself authorize applying them or importing realms into a live instance.

## Core Concepts

- **Realm**: An isolated tenant with its own users, clients, roles, and settings. The `master` realm is for Keycloak administration only — never use it for applications.
- **Client**: An application registered in a realm that uses Keycloak for auth (OIDC, SAML).
- **Build options**: Persisted configuration baked at image/build time (e.g., `--db`). Stored in plain text — never use for secrets.
- **Runtime options**: Applied at startup (e.g., `--db-password`). Support keystore-based secret storage.
- **Two startup modes**:
  - `start-dev` — development mode, insecure defaults, HTTP only, no hostname required. **Never use in production.**
  - `start` — production mode, TLS mandatory, hostname required.

## Configuration Sources (Priority: Highest → Lowest)

| Source | Format | Example |
|---|---|---|
| CLI parameters | `--<key>=<value>` | `--db=postgres` |
| Environment variables | `KC_<KEY>=<value>` | `KC_DB=postgres` |
| `conf/keycloak.conf` | `<key>=<value>` | `db=postgres` |
| Java KeyStore | PKCS12 password entry | `kc.db-password` |

### Interpolation in `keycloak.conf`

```ini
# Reference env var with optional default
hostname=${HOSTNAME_ENV_VAR:localhost}

# Escape literal dollar sign
some-option=\$literal
```

## Common Workflows for Coding Agents

When helping users with Keycloak:

1. **Identify the deployment target first**: Docker (single node dev/prod), Docker Compose (full stack), Kubernetes (plain manifests), or Kubernetes with Operator.
2. **Separate build vs. runtime options**: Build options (`--db`, `--health-enabled`, `--features`) belong in `Dockerfile`/`kc.sh build`. Runtime options (credentials, hostname, TLS certs) go in environment variables or `keycloak.conf`.
3. **Never put secrets in build options**: They are stored in plain text in the image layer.
4. **Always confirm TLS for production**: `start-dev` implies HTTP — production requires certificates.
5. **Prefer optimized images**: Run `kc.sh build` in Dockerfile so `start --optimized` skips build at container startup.
6. **For Kubernetes Operator**: Always provision the database first; the Operator does not create databases.
7. **Check hostname strictly**: Most token/redirect issues trace back to hostname misconfiguration. Use `--hostname-debug=true` to diagnose.
8. **For realm export/import**: Use CLI export (not Admin Console export) for backup — console export masks secrets.
9. **Memory**: Always set container memory limits. Default heap is 70% of container RAM.
10. **Cluster minimum**: Recommend 2+ instances for any production deployment.

## Detailed reference

Consult [references/guide.md](references/guide.md) for the following topics. Search for the relevant heading and read that section as needed:

- Docker
- Kubernetes — Getting Started (Minikube)
- Kubernetes — Keycloak Operator
- Realm Import with Operator
- Database Configuration
- Hostname Configuration
- TLS / HTTPS
- Reverse Proxy
- Health Checks
- Import / Export Realms (CLI)
- Production Checklist
- Key Environment Variables Reference
- CLI Command Reference
- IPv4 / IPv6 Explicit Configuration
