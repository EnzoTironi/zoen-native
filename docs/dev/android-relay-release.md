# Android relay release dependency

The Android port uses the current shared Rust core. That core requires relay protocol 4, including explicit device enrollment and authenticated history handling. The Apple app built from the same source has the same requirement.

At 2026-10-09 23:11 UTC, a certificate-only Hello to `wss://relay.tryzoen.com/v1/sync` returned protocol 2. The probe sent neither Auth nor Register and created no account. `/healthz` returned `ok`; health alone does not establish client compatibility. The isolated local relay is rebuilt from the same source as the Android libraries for encrypted device journeys.

## Existing deployment

`relay.tryzoen.com` routes to the existing Fly app `zoen-staging-relay`, also serving `api.tryzoen.com`, `media.tryzoen.com` and `id.tryzoen.com`. Read-only Fly status reported one started `gru` machine with its readiness check passing:

| Field | Observed value |
|---|---|
| Machine | `7817964c177d68` |
| Version | `7` |
| Image | `zoen-staging-relay:deployment-01M4FN7VVAF967S009B2W0V8HR` |
| Last updated | `2026-10-09T06:25:09Z` |

## Proposed server action

After the current-source Rust and Android checks pass, deploy only the existing relay image from the reviewed checkout:

```bash
fly deploy . --config infra/fly/relay.toml \
  --dockerfile infra/fly/relay.Dockerfile \
  --app zoen-staging-relay --ha=false --remote-only --yes
```

This command targets the existing relay, PostgreSQL and FoundationDB configuration. It does not create the separate paid agent sandbox app. The relay applies its existing SQL migrations on startup.

The upgrade affects every client of the public relay. Protocol-2/3 clients must update to the current shared core; the new relay deliberately refuses them before authentication. Changing that shared public service requires a separate deployment decision. No public deployment has been performed as part of the Android source port.

After deployment, check `/healthz`, `/readyz`, the Fly readiness check and an unauthenticated protocol-4 Hello. Isolated two-account Android journeys establish encrypted chat, attachment and recovery behavior; they do not establish a public rollout result. Keep the previous image identity above in the deployment record. Switching an image alone does not undo database migrations or newly written history.

See [Android verification](android.md) and [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41) for the current build, native tests and visual evidence.
