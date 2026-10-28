# Security model

This document records the trust boundaries and the controls that enforce them.
When a control changes, update this file and its tests.

## Trust boundaries

| Boundary | Untrusted input | Control |
| --- | --- | --- |
| Browser → `peroxo` `/ws?token=` | bearer token, JSON frames | token verified over gRPC; frame size, content, and rate limits; tenant pinned from the token |
| Browser → `peroxo` `/conversations` | `project_id`, `user_id_1`, `user_id_2` | project must match the token; caller must be one of the two users; ids bounded |
| Client → `auth-service` HTTP | Google ID token, project/user ids, tenant secret | Google token verification, constant-time secret compare, per-IP/project rate limits, 16 KiB body cap |
| `peroxo` → `auth-service` gRPC | token | shared bearer secret (`GRPC_SHARED_SECRET`) |
| `peroxo` → `chat-service` gRPC | project/user ids, message bodies, conversation ids | shared bearer secret; server-side conversation resolution; participant checks |
| `chat-service` → Scylla / `auth-service` → Postgres+Redis | query parameters | bound/prepared statements; credentials from environment |

## Access-control rules

1. **Tenants are derived, never trusted.** `project_id` and `user_id` come from
   the verified token. Client-supplied `project_id` values are ignored.
2. **A conversation id is not a capability.** On write, chat-service resolves the
   conversation from `(project_id, sender_id, receiver_id)` and ignores the
   client's id. On read, it checks `conversation_members` for the authenticated
   caller.
3. **Only participants can resolve a pair.** `GET /conversations` and
   `GetSertConversation` require the caller to be one of the two user ids.
4. **Rooms are project-wide.** Any member of a project may join a room in that
   project and receive its traffic. This is intentional; if rooms become
   private, they need an explicit membership table and the same participant
   check as DMs.

## Data protection

- Tokens are 24-character random strings stored in Redis with a 600 s TTL.
- Logs redact tokens and credential-bearing URLs (`auth-service/src/redact.rs`).
- `Cache-Control: no-store` is set on auth responses; security headers are set
  on both HTTP surfaces.
- Secrets live in `.env` / the platform secret store, never in images or git
  (`docs/SECRETS.md`).

## Known limitations

- **No TLS in-process.** `auth-service` (3004) and `peroxo` (4001) expect a TLS
  terminator in front. HSTS is emitted but only takes effect over HTTPS.
- **Bearer tokens in query strings.** Browser WebSockets cannot set an
  `Authorization` header, so `/ws?token=` is supported. Tokens are redacted from
  logs; a short-lived single-use ticket exchange is the next step.
- **Room membership is not authorized.** See rule 4.
- **Legacy conversations need backfill.** Run chat-service once with
  `BACKFILL_CONVERSATION_MEMBERS=1` after deploying the membership table, or
  pre-existing conversations will be unreadable by their participants.
- **The optional `mongo_db` feature is legacy.** Its conversation ids are
  derived from user ids only, not scoped by project, and its read path
  authorizes from sender/recipient rows rather than `conversation_members`.
  Do not enable it for multi-tenant deployments.
- **gRPC is bearer-secret, not mTLS.** Anything that can read the secret can
  impersonate a service. Rotate it like any other credential.
- **Rate limits key on the peer address.** Behind a reverse proxy set
  `TRUST_PROXY_HEADERS=1` so the limiter uses the client address from
  `X-Forwarded-For`; only do this when the proxy overwrites the header.

## Verification checklist

Run before a release:

1. `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings -A clippy::result_large_err`, and `cargo test --locked` in each service.
2. `cargo audit` in each service (CI does this).
3. Abuse cases, with a real token:
   - user A cannot fetch history for a conversation between B and C;
   - a forged `conversation_id` on `SendDirectMessage` is ignored and the
     message lands in the A↔B thread;
   - `GET /conversations` for a pair the caller is not part of returns 403;
   - a frame over 64 KiB is rejected;
   - a burst above ~30 msg/s closes the session;
   - `GET /metrics` on the public port returns 404;
   - `curl -I` shows the security headers.
4. `docker compose config` succeeds and refuses to start without
   `GRPC_SHARED_SECRET`, `REDIS_PASSWORD`, and `GRAFANA_ADMIN_PASSWORD`.
5. Grep the logs of an end-to-end run for `pxtok_` and the database password:
   no matches.
