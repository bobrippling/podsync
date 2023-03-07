# podsync

A HTTP server for syncing podcast app state, mirroring the gpodder API. Designed for use with [AntennaPod].

[AntennaPod]: https://github.com/AntennaPod/AntennaPod

# Building

podsync uses sqlx in [offline mode] for builds.

To update the schema:
- `cargo install sqlx-cli`
- `cargo sqlx prepare` (with a present database)
- Commit in `sqlx-data.json`
- Unset `DATABASE_URL` during compilation

[offline mode]: https://docs.rs/sqlx/latest/sqlx/macro.query.html#offline-mode-requires-the-offline-feature
