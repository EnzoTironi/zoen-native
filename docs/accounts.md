# Accounts created for the project

Every external account an agent creates for Zoen is listed here. No secrets in this file;
keys live in the gitignored `.env` or as secrets on the service.

| date | service | inbox | purpose | plan |
|---|---|---|---|---|
| 2026-10-08 | Fly.io apps `zoen-staging-relay`, `zoen-staging-pg` | Enzo's existing Fly account (org `personal`), not a new sign-up | staging relay and Postgres (ADR 0015) | pay as you go, ≈ $10/month |
| 2026-10-08 | Tigris bucket `zoen-staging-media` | created through `fly storage create` on Enzo's Fly account | encrypted media blobs | free tier (5 GB) |
| 2026-10-08 | Cloudflare DNS records `relay`, `api`, `media`, `id` on tryzoen.com | Enzo's Cloudflare account, scoped API token from Enzo | staging hostnames (OpenTofu) | free |
