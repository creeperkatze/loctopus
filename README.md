# Loctopus

A minimal backend counting lines of code for a public repo on GitHub, Codeberg, GitLab, or
Bitbucket.

[![GitHub Issues](https://img.shields.io/github/issues/creeperkatze/loc)](https://github.com/creeperkatze/loc/issues)
[![GitHub Pull Requests](https://img.shields.io/github/issues-pr/creeperkatze/loc)](https://github.com/creeperkatze/loc/pulls)
[![GitHub Repo stars](https://img.shields.io/github/stars/creeperkatze/loc?style=flat)](https://github.com/creeperkatze/loc/stargazers)

[🐳 Package](https://github.com/creeperkatze/loc/pkgs/container/loctopus)

It downloads the repo's source as a tarball, walks the files, and builds a directory tree of line
counts broken down by file extension.

## 🚀 Run

```bash
cargo run
```

For dev mode with auto-reload on file changes (rebuilds and restarts the server whenever you save):

```bash
cargo install cargo-watch  # one-time
cargo dev
```

Listens on `http://0.0.0.0:3000` by default (override with the `PORT` env var). Optionally set
`GITHUB_TOKEN` / `CODEBERG_TOKEN` / `GITLAB_TOKEN` / `BITBUCKET_TOKEN` to raise API rate limits and
access private repos you have access to. Set `RUST_LOG=debug` for more verbose logging (defaults to
`info`).

## 📖 API

`:platform` is `github`, `codeberg`, `gitlab`, or `bitbucket`.

### `GET /`

Returns server status.

### `GET /:platform/:owner/:repo`

Returns the full line-count tree, e.g. `/github/modrinth/code` or `/codeberg/ziglang/zig`.

Query params:
- `branch`: defaults to the repo's default branch.
- `filter`: comma-separated regexes matched against each file's extension key (e.g. `.ts$,.tsx$`)
  to only count matching files.

```json
{
  "loc": 42,
  "locByLangs": { ".rs": 40, "Dockerfile": 2 },
  "children": {
    "main.rs": 40,
    "Dockerfile": 2
  }
}
```

Folders are nested objects with the same shape; files are plain numbers (their line count).

### `GET /:platform/:owner/:repo/badge`

Same query params as above, plus `format=human` to abbreviate the count (e.g. `1.2k`). Returns a
[shields.io endpoint badge](https://shields.io/badges/endpoint-badge) payload:

```json
{ "schemaVersion": 1, "label": "lines", "message": "42", "cacheSeconds": 86400 }
```

## 🐳 Deploy

Published to GHCR on every `v*` tag push. On your VPS:

**1. Create `docker-compose.yml`**

```yaml
services:
  loctopus:
    image: ghcr.io/creeperkatze/loctopus:latest
    restart: unless-stopped
    env_file: .env
    ports:
      - "3000:3000"
    volumes:
      - ./data:/app/data
```

**2. Create `.env`** (see [.env.example](.env.example), every var is optional)

**3. Start**

```bash
docker compose up -d
```

To build and run locally instead of pulling the published image, use the `docker-compose.yml` in
this repo (`docker compose up -d --build`).

## 💾 Caching

Results are cached per `(platform, owner, repo, branch, filter)` for 24 hours: an in-memory layer
for hot lookups, backed by a SQLite database (`data/cache.sqlite`) so entries survive restarts.
Expired rows are swept opportunistically on writes.

## 📝 Notes

This is intentionally minimal: no rate limiting, no repo size limits, and binary files are skipped
via a simple null-byte heuristic rather than full content-type detection.

## 📜 License

AGPL-3.0
