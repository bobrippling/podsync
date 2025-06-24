# podsync

A HTTP server for syncing podcast app state, mirroring the [gpodder API]. Designed for use with [AntennaPod]'s [sync service].

[gpodder API]: https://github.com/gpodder/mygpo/blob/80c41dc0c9a58dc0e85f6ef56662cdfd0d6e3b16/doc/api/reference/events.rst
[AntennaPod]: https://github.com/AntennaPod/AntennaPod
[sync service]: https://github.com/AntennaPod/AntennaPod/blob/24d1a06662c8eec31f3a4c3ebdcd3aea759fb63a/core/src/main/java/de/danoeh/antennapod/core/sync/SyncService.java

# Setup

## Creating Users

Add a user with `scripts/add-user.sh`:

```sh
$ ./scripts/add-user.sh yourname
yourname's pass: <enter password>
```

This will add a user into the database, assumed to be `pod.sql`.

# Endpoints

podsync doesn't cover the [full gpodder API], just enough to get AntennaPod to work:

- auth:
	- `POST api/2/auth/{username}/login.json`
	- `POST api/2/auth/{username}/logout.json`
- devices:
	- `GET api/2/devices/{username}.json`
	- `POST api/2/devices/{username}/{device}.json`
- subscriptions:
	- `GET api/2/subscriptions/{username}/{device}.json`
	- `POST api/2/subscriptions/{username}/{device}.json`
- episodes:
	- `GET api/2/episodes/{username}.json`
	- `POST api/2/episodes/{username}.json`

[full gpodder API]: https://github.com/gpodder/mygpo/tree/80c41dc0c9a58dc0e85f6ef56662cdfd0d6e3b16/doc/api/reference

# Logging

podsync uses the `RUST_LOG` environment variable for logging. To generate logs similar to a webserver:
```sh
export RUST_LOG=podsync=info

# or for debugging:
export RUST_LOG=podsync=trace

# for warp/endpoint output:
export RUST_LOG=podsync=info,warp=info
```

See the [log crate] for more details

[log crate]: https://crates.io/crates/log

# Building

## Modes

podsync has two backends: SQL database or plain text files. The former being more scalable, the latter being easier to inspect and manipulate with Unix tools.

By default it builds in file mode, to build in sql mode, build with `cargo build --features backend-sql`.

## SQLx offline build

podsync uses sqlx in [offline mode] for builds (see [`build.rs`](./build.rs) for more).

To update the schema:
```sh
export DATABASE_URL=sqlite://pod.sql
cargo install sqlx-cli
cargo sqlx prepare -- --tests --features backend-sql
git commit -m 'Update sqlx snapshot' sqlx-data.json
```

[offline mode]: https://docs.rs/sqlx/latest/sqlx/macro.query.html#offline-mode-requires-the-offline-feature
