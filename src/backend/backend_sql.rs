use std::future::Future;
use std::path::{Path, PathBuf};

use sqlx::{migrate::MigrateDatabase, query, query_as, Pool, Sqlite, SqlitePool, Transaction};

use log::{error, info};

use crate::backend::FindError;
use crate::device::{DeviceAndSub, DeviceUpdate};
use crate::episode::{Episode, EpisodeRaw};
use crate::podsync::{QueryEpisodes, Url};
use crate::subscription::SubscriptionChangesFromClient;
use crate::user::User;
use crate::Timestamp;

type Result<T> = std::result::Result<T, ()>;

pub struct Backend(pub Pool<Sqlite>);

fn into_sql(path: &Path) -> PathBuf {
    path.join("pod.sql")
}

pub async fn init(data_dir: &Path) {
    let final_path = format!(
        "sqlite://{}",
        into_sql(data_dir).to_str().expect("non utf-8 data")
    );
    match Sqlite::create_database(&final_path).await {
        Ok(()) => {
            info!("Using {}", &final_path);
        }
        Err(e) => {
            let sqlx::Error::Database(db_err) = e else {
                panic!("error creating database: {e}");
            };

            panic!("sql db error: {db_err:?}"); //.code()
        }
    }
}

impl Backend {
    pub async fn new(data_dir: &Path) -> Self {
        let db_pathbuf = into_sql(data_dir);
        let db_path = db_pathbuf.to_str().expect("non utf-8 data");
        let pool = match SqlitePool::connect(db_path).await {
            Ok(pool) => pool,
            Err(_err) => {
                init(data_dir).await;
                SqlitePool::connect(db_path).await.expect("db connection")
            }
        };

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migration");

        Self(pool)
    }
}

impl Backend {
    async fn transact<'t, T, R, F>(&self, transaction: T) -> Result<R>
    where
        T: FnOnce(Transaction<'t, Sqlite>) -> F,
        F: Future<Output = Result<(Transaction<'t, Sqlite>, R)>>,
    {
        let tx = self.0.begin().await.map_err(|e| {
            error!("error beginning transaction: {:?}", e);
        })?;

        // could probably pass &mut *tx here
        let (tx, r) = transaction(tx).await?;

        tx.commit().await.map_err(|e| {
            error!("error committing transaction: {:?}", e);
        })?;

        Ok(r)
    }
}

impl Backend {
    pub async fn find_user(&self, username: &str) -> std::result::Result<User, FindError> {
        query_as!(
            User,
            "
                SELECT *
                FROM users
                WHERE username = ?
                ",
            username,
        )
        .fetch_one(&self.0)
        .await
        .map_err(|e| {
            if matches!(e, sqlx::Error::RowNotFound) {
                FindError::NotFound
            } else {
                FindError::Internal
            }
        })
    }

    /// session_id: set to None to logout / make NULL
    pub async fn update_user(&self, username: &str, session_id: Option<&str>) -> bool {
        query!(
            "
            UPDATE users
            SET session_id = ?
            WHERE username = ?
            ",
            session_id,
            username,
        )
        .execute(&self.0)
        .await
        .map_err(|e| {
            error!("update user: {e}");
            e
        })
        .is_ok()
    }

    pub async fn users_with_session(&self, session_id: &str) -> Result<Vec<User>> {
        query_as!(
            User,
            "
            SELECT *
            FROM users
            WHERE session_id = ?
            ",
            session_id,
        )
        .fetch_all(&self.0)
        .await
        .map_err(|e| {
            error!("couldn't query for session {session_id}: {e:?}");
        })
    }
}

impl Backend {
    pub async fn devices_for_user(&self, username: &str) -> Result<Vec<DeviceAndSub>> {
        query_as!(
            DeviceAndSub,
            r#"
            SELECT id, caption as "caption!: _", type as "type!: _", COUNT(*) as "subscriptions!: _"
            FROM devices
            INNER JOIN subscriptions
                ON devices.username = subscriptions.username
            GROUP BY devices.username, devices.id
            HAVING devices.username = ?
            "#,
            username,
        )
        .fetch_all(&self.0)
        .await
        .map_err(|e| {
            error!("error selecting devices: {:?}", e);
        })
    }

    pub async fn update_device(
        &self,
        username: &str,
        device_id: &str,
        update: DeviceUpdate,
    ) -> Result<()> {
        let caption = update.caption;
        let r#type = update.r#type;
        let type_default = r#type.clone().unwrap_or_default();

        query!(
            "
            INSERT INTO devices
            (id, username, caption, type)
            VALUES
            (?, ?, ?, ?)
            ON CONFLICT
            DO
                UPDATE SET
                    caption = coalesce(?, devices.caption),
                    type = coalesce(?, devices.type)
                WHERE id = ? AND username = ?
            ",
            device_id,
            username,
            caption,
            type_default,
            caption,
            r#type,
            device_id,
            username
        )
        .execute(&self.0)
        .await
        .map(|_| ())
        .map_err(|e| {
            error!("error inserting device: {:?}", e);
        })
    }
}

impl Backend {
    pub async fn subscriptions(
        &self,
        username: &str,
        device_id: &str,
        since: Timestamp,
    ) -> Result<Vec<Url>> {
        query_as!(
            Url,
            r#"
            SELECT url,
                deleted as "deleted: _",
                created as "created!: _"
            FROM subscriptions
            WHERE username = ?
                AND device = ?
                AND (
                    created > ? OR deleted > ?
                )
            "#,
            username,
            device_id,
            since,
            since,
        )
        .fetch_all(&self.0)
        .await
        .map_err(|e| {
            error!("error selecting subscriptions: {e:?}");
        })
    }

    pub async fn update_subscriptions(
        &self,
        username: &str,
        device_id: &str,
        changes: &SubscriptionChangesFromClient,
        now: Timestamp,
    ) -> Result<()> {
        self.transact(|mut tx| async {
            for url in &changes.remove {
                query!(
                    "
                    UPDATE subscriptions
                    SET
                        deleted = ?
                    WHERE username = ?
                        AND device = ?
                        AND url = ?
                        AND deleted IS NULL
                    ",
                    now,
                    username,
                    device_id,
                    url,
                )
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    error!("error deleting (updating) subscription: {e:?}");
                })?;
            }

            for url in &changes.add {
                query!(
                    "
                    INSERT INTO subscriptions
                    (username, device, url, created)
                    VALUES
                    (?, ?, ?, ?) -- `deleted` <- NULL
                    ON CONFLICT
                    DO NOTHING
                    ",
                    username,
                    device_id,
                    url,
                    now,
                )
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    error!("error inserting subscription: {e:?}");
                })?;
            }

            Ok((tx, ()))
        })
        .await?;

        info!(
            "{username} on {device_id}, added {} subscriptions, removed {}, timestamp {now}",
            changes.add.len(),
            changes.remove.len()
        );

        Ok(())
    }
}

impl Backend {
    pub async fn episodes(&self, username: &str, query: &QueryEpisodes) -> Result<Vec<EpisodeRaw>> {
        let since = query.since.unwrap_or_else(Timestamp::zero);
        let podcast_filter = &query.podcast;
        let device_filter = &query.device;
        // query.aggregated: unique on (sub, episode)-tuple - always true with how we store

        query_as!(
            EpisodeRaw,
            r#"
            SELECT episodes.podcast, episode,
                guid, episodes.device,
                timestamp as "timestamp: _",
                action as "action!: _",
                started, position, total,
                modified as "modified?: _"
            FROM
                episodes,
                (SELECT ? as podcast, ? as device) as filter
            WHERE username = ?
                AND modified > ?
                AND (filter.podcast IS NULL OR filter.podcast = episodes.podcast)
                AND (filter.device IS NULL OR filter.device = episodes.device)
            "#,
            podcast_filter,
            device_filter,
            username,
            since,
        )
        .fetch_all(&self.0)
        .await
        .map_err(|e| {
            error!("error selecting episodes: {e:?}");
        })
    }

    pub async fn update_episodes(
        &self,
        username: &str,
        now: Timestamp,
        changes: Vec<Episode>,
    ) -> Result<()> {
        self.transact(|mut tx| async {
            for change in changes {
                let hash = change.hash();

                let EpisodeRaw {
                    podcast,
                    episode,
                    timestamp,
                    guid,
                    action,
                    started,
                    position,
                    total,
                    device,
                    modified: _,
                } = change.into();

                query!(
                    "
                    INSERT INTO episodes
                    (
                        username, device,
                        podcast, episode,
                        timestamp, guid,
                        action,
                        started, position, total,
                        modified
                    )
                    VALUES
                    (
                        ?, ?,
                        ?, ?,
                        ?, ?,
                        ?,
                        ?, ?, ?,
                        ?
                    )
                    ON CONFLICT
                    DO
                        UPDATE SET
                            timestamp = coalesce(?, episodes.timestamp),
                            guid = coalesce(?, episodes.guid),
                            action = coalesce(?, episodes.action),
                            started = coalesce(?, episodes.started),
                            position = coalesce(?, episodes.position),
                            total = coalesce(?, episodes.total),
                            modified = ?,
                            content_hash = ?
                        -- only update if we've changed the contents
                        WHERE content_hash <> ?
                    ",
                    // values
                    username,
                    device,
                    podcast,
                    episode,
                    timestamp,
                    guid,
                    action,
                    started,
                    position,
                    total,
                    now,
                    // update
                    timestamp,
                    guid,
                    action,
                    started,
                    position,
                    total,
                    now,
                    hash,
                    // update where
                    hash,
                )
                .execute(&mut tx)
                .await
                .map_err(|e| {
                    error!("error querying mid-transaction: {:?}", e);
                })?;
            }

            Ok((tx, ()))
        })
        .await
    }
}

#[cfg(test)]
pub mod test {
    use sqlx::{migrate::MigrateDatabase, Pool, Sqlite, SqlitePool};

    pub async fn create_db() -> Pool<Sqlite> {
        let url = ":memory:";

        Sqlite::create_database(url).await.unwrap();

        let db = SqlitePool::connect(url).await.unwrap();

        sqlx::migrate!("./migrations").run(&db).await.unwrap();

        db
    }
}
