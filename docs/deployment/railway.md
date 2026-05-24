# Railway Deployment

## Prerequisites

- [Railway CLI](https://docs.railway.app/railway-cli) installed and authenticated.
- A Railway team (project `topic-tree-with-qav` should already exist under team `clankercode`).

## Initial Setup

Run once to create the project, volume, and environment:

```bash
just railway-init
```

This creates:
- A Railway project named `topic-tree-with-qav`
- A persistent volume mounted at `/data`
- Environment variables for the database path and logging

## Deploying

Deploy the current commit:

```bash
just railway-deploy
```

Or manually:

```bash
railway up --detach
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_PATH` | `/data/app.db` | SQLite database path |
| `PORT` | `3000` | HTTP server port |
| `RUST_LOG` | `info` | Logging level |

## Accessing Logs

```bash
just railway-logs
```

## Opening the App

```bash
just railway-open
```

## Production URL

The production URL is assigned by Railway. Check the Railway dashboard or run `railway open` to find it.

## Updating

Push a new commit to `main` and run `just railway-deploy`. Railway automatically deploys the new image on subsequent runs of `railway up`.

## Volume Persistence

The SQLite database is stored on a Railway volume at `/data`. This persists across deploys. If you delete the volume, data is lost.

## Known Limitations

- Single-instance only (volume = single Railway instance).
- No automatic backup (see `just db-dump` for manual backups).
