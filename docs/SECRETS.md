# Secret handling

PerOXO services read all credentials from the environment. `.env` files are for
local development only, are gitignored, and must never be copied into an image,
a backup, or a support bundle.

## Rules

- **Never commit a secret.** `.env`, `.env.*`, `*.pem`, `*.key`, `*.p12`, and
  `secrets/` are ignored; keep it that way.
- **Rotate first, purge second.** A credential that reached a remote (or a
  plaintext file on a shared disk) is compromised the moment it lands. Rotate it
  in the provider console, then update the environment.
- **Redact in logs.** Tokens, API keys, and credential-bearing URLs must never be
  logged at any level. Code that needs an identifier should log a prefix and a
  hash, never the value.
- **One environment per deployment.** Use the platform's secret store (Docker
  secrets, Kubernetes Secrets, or the CI secret store) for shared deployments;
  `.env` is a developer convenience.

## Required variables

| Variable | Service | Notes |
| --- | --- | --- |
| `DATABASE_URL` | auth-service, user-service | Postgres connection string, includes the password |
| `REDIS_URL` | auth-service | `redis://:password@host:6379` when Redis auth is enabled |
| `GOOGLE_CLIENT_ID` | auth-service | Not secret, but deployment-specific |
| `ALLOWED_ORIGINS` | auth-service | Comma-separated HTTPS origins, no wildcard |
| `GRPC_SHARED_SECRET` | peroxo, auth-service, chat-service | Service-to-service bearer secret (see below) |
| `GRAFANA_ADMIN_PASSWORD` | docker compose | No default; compose refuses to start without it |
| `REDIS_PASSWORD` | docker compose | Passed to Redis via `requirepass`; use URL-safe characters (or percent-encode them) because it is embedded in `REDIS_URL` |
| `GRAFANA_ADMIN_USER` / `GRAFANA_ADMIN_PASSWORD` | docker compose | No default password; compose refuses to start without it |
| `TRUST_PROXY_HEADERS` | auth-service | Set to `1` only behind a trusted proxy that rewrites `X-Forwarded-For` |

## Rotation runbook

1. Create the replacement credential in the provider (Neon, Atlas, Grafana, Redis).
2. Roll it out to the running services and verify health.
3. Revoke the old credential.
4. Search for the old value on disk and in logs: `grep -rn '<value>' .` and the
   log store. If it appears anywhere, treat the rotation as incomplete.
5. Record the rotation date in the team runbook.

## If a secret is committed

1. Rotate it immediately — history purging does not un-leak it.
2. Remove it from the working tree and add an ignore rule.
3. Rewrite history only after rotation, and coordinate with everyone who has a
   clone.
4. Run `gitleaks detect` over the full history to confirm the tree is clean.
