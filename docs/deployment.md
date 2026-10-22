# Deployment

## Build and release

CI deploys the website and Dojo worker to Fly.io after their required checks pass
on `main`. [fly.toml](../fly.toml) and [dojo/fly.toml](../dojo/fly.toml) define the
apps. CI uses the prebuilt Dockerfiles; the standard Dockerfiles build from source
for manual deployments.

Both CI deployments share the [packaged compatibility gate](packaged-compatibility.md),
including site E2E/Docker checks and the packaged worker's real proof checkers. That
document describes its coverage limits and the additional branch-protection check.

Deploy the website and worker from the same revision. For separate manual
deployments, update the worker first.

For a deployment outside Fly, set `LEPTOS_ENV=PROD`, build with
`cargo leptos build --release`, and serve `target/site` with `target/release/hashi`.

## Configuration

Store credentials in the deployment platform's secret store.

- `HASHI_DAILY_SECRET` and `DOJO_SECRET`: independent random values of at least
  32 characters. Preserve existing production values across deployments;
  changing them changes future daily puzzles or seasonal benchmark sets.
- `DOJO_URL`: worker URL. Leave unset when solver evaluation is disabled.
- `DOJO_TOKEN`: at least 32 random characters, matching on the website and worker
  when evaluation is enabled.
- `HASHI_ADMINS`: comma-separated ids of existing registered accounts. Register
  the accounts before assigning their ids, and remove grants before deleting an
  admin account.
- `HASHI_ADMIN_TOKEN`: optional credential for administrative scripts. House-entry
  uploads from CI require the matching GitHub Actions secret.
- `TURSO_DATABASE_URL` and `TURSO_AUTH_TOKEN`: remote database connection. For a
  local database, set `HASHI_DB_PATH` to a persistent location; it defaults to
  `hashi.db`.
- `HASHI_APP_MIN_VERSION`: optional, the oldest app version (`x.y.z`) the server
  answers; older apps are asked to update (see [mobile.md](mobile.md#api-headers)).
  Raise it only when a server function changes incompatibly, and only after the
  stores have offered the update for a while: installed apps update late.
- `HASHI_APP_ORIGINS`: optional, comma-separated origins of the app's WebView that
  may call the API. Defaults to `https://tauri.localhost,tauri://localhost`.

A malformed value of either stops the server at startup.

The worker also needs `APP_URL` pointing to the website. Its remaining settings
are documented in [dojo/fly.toml](../dojo/fly.toml).

## Maintenance

Back up production data before schema-changing deployments and verify that the
backup can be restored. For local SQLite, use a consistent database backup or stop
the server before copying it; copying the main file during WAL writes can omit data.

Increment `DOJO_SEASON` to start a new season and archive the previous one.
Keep `DOJO_SECRET` stable through this transition.

Changes to judging, fuel costs, or limits that affect scores require a
`dojo::RULES_VERSION` bump. Changes to generated benchmark sets also require a
`dojo::GENERATOR_VERSION` bump.

On Fly, client addresses come from its proxy. Direct deployments use the socket
peer; another reverse proxy requires an explicit trusted-proxy integration for
client-address handling.
