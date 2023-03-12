use std::str::FromStr;
use std::{future::Future, result, sync::Arc};

use log::{error, info, trace};
use serde::{Deserialize, Serialize};
use sqlx::{query, query_as, Pool, Sqlite, Transaction};
use warp::http;

use crate::auth::{AuthAttempt, SessionId};
use crate::device::{DeviceAndSub, DeviceUpdate};
use crate::episode::{Episode, EpisodeRaw, Episodes};
use crate::subscription::{SubscriptionChangesFromClient, SubscriptionChangesToClient};
use crate::time::Timestamp;
use crate::user::User;

pub struct PodSync(Pool<Sqlite>);

pub struct PodSyncAuthed<const USER_MATCH: bool = false> {
    sync: Arc<PodSync>,
    session_id: SessionId,
    username: String,
}

#[derive(Debug, Serialize)]
pub struct UpdatedUrls {
    // important: this timestamp is used by future client synchronisations
    timestamp: Timestamp,
    // unused by antennapod
    update_urls: Vec<(String, String)>,
}

#[derive(Debug, Deserialize)]
pub struct QueryEpisodes {
    since: Option<Timestamp>,
    #[allow(dead_code)]
    aggregated: Option<bool>,
    podcast: Option<String>,
    device: Option<String>,
}

#[derive(Debug)]
pub enum Error {
    Internal,
    Unauthorized,
    BadRequest,
}

pub type Result<T> = result::Result<T, Error>;

impl Into<http::StatusCode> for Error {
    fn into(self) -> http::StatusCode {
        match self {
            Self::Internal => http::StatusCode::INTERNAL_SERVER_ERROR,
            Self::Unauthorized => http::StatusCode::UNAUTHORIZED,
            Self::BadRequest => http::StatusCode::BAD_REQUEST,
        }
    }
}

impl warp::reject::Reject for Error {}

impl PodSync {
    pub fn new(db: Pool<Sqlite>) -> Self {
        Self(db)
    }

    pub async fn login(
        self: &Arc<Self>,
        auth_attempt: AuthAttempt,
        client_session_id: Option<SessionId>,
    ) -> Result<PodSyncAuthed> {
        let username = auth_attempt.user();

        let user = query_as!(
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
                error!("rejecting non-existant user {}", username);
                Error::Unauthorized
            } else {
                error!("couldn't authenticate user {}: {e:?}", username);
                Error::Internal
            }
        })?;

        if auth_attempt.calc_pwhash() != user.pwhash {
            error!("wrong password for user {}", username);
            return Err(Error::Unauthorized);
        }

        let ok = |session_id| {
            Ok(PodSyncAuthed {
                sync: Arc::clone(self),
                session_id,
                username: auth_attempt.user().to_string(),
            })
        };

        let db_session_id = match user.session_id {
            Some(ref id) => {
                let session_id = SessionId::from_str(&id).map_err(|()| {
                    error!("invalid session_id in database: {:?}", user.session_id);
                    Error::Internal
                })?;
                Some(session_id)
            }
            None => None,
        };

        match (client_session_id, db_session_id) {
            (None, None) => {
                // initial login
                let session_id = SessionId::new();
                let str = session_id.to_string();

                query!(
                    "
                    UPDATE users
                    SET session_id = ?
                    WHERE username = ?
                    ",
                    str,
                    username,
                )
                .execute(&self.0)
                .await
                .map_err(|e| {
                    error!("couldn't login user {}: {e:?}", username);
                    Error::Internal
                })?;

                info!("{username} login: new session created");
                ok(session_id)
            }
            (Some(client), Some(db_id)) => {
                if client == db_id {
                    info!("{username} login: session check passed");
                    ok(client)
                } else {
                    info!("{username} login: session check failed");
                    Err(Error::Internal)
                }
            }
            (Some(_), None) => {
                // logged out but somehow kept their token?
                info!("{username} login: no session in db");
                Err(Error::Unauthorized)
            }
            (None, Some(db_id)) => {
                // logging in again, client's forgot their token
                info!("{username} login: fresh login");
                ok(db_id)
            }
        }
    }

    pub async fn authenticate(self: &Arc<Self>, session_id: SessionId) -> Result<PodSyncAuthed> {
        let session_str = session_id.to_string();

        let users = query_as!(
            User,
            "
            SELECT *
            FROM users
            WHERE session_id = ?
            ",
            session_str,
        )
        .fetch_all(&self.0)
        .await
        .map_err(|e| {
            error!("couldn't query for session {session_id}: {e:?}");
            Error::Internal
        })?;

        match &users[..] {
            [] => {
                error!("no user found for session {session_id}");
                Err(Error::Unauthorized)
            }
            [user] => {
                assert_eq!(user.session_id, Some(session_str));

                Ok(PodSyncAuthed {
                    sync: Arc::clone(self),
                    session_id,
                    username: user.username.clone(),
                })
            }
            _ => {
                error!("multiple users found for session {session_id}");
                Err(Error::Internal)
            }
        }
    }
}

impl PodSyncAuthed {
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn with_user(self, username: &str) -> Result<PodSyncAuthed<true>> {
        if username == self.username {
            Ok(PodSyncAuthed {
                sync: self.sync,
                session_id: self.session_id,
                username: self.username,
            })
        } else {
            error!(
                "mismatching session & username: session={{ username: {}, session_id: {} }}, username={username}",
                self.username,
                self.session_id,
            );
            Err(Error::Unauthorized)
        }
    }
}

impl PodSyncAuthed<true> {
    pub async fn logout(&self) -> Result<()> {
        let username = &self.username;
        info!("{username} logout");

        query!(
            "
                UPDATE users
                SET session_id = NULL
                WHERE username = ?
                ",
            username,
        )
        .execute(&self.sync.0)
        .await
        .map(|_| ())
        .map_err(|e| {
            error!("error updating session_id: {e:?}");
            Error::Internal
        })
    }

    pub async fn devices(&self) -> Result<Vec<DeviceAndSub>> {
        let username = &self.username;
        trace!("{username} getting devices");

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
        .fetch_all(&self.sync.0)
        .await
        .map(|devs| {
            info!("{username}, {} devices", devs.len());
            devs
        })
        .map_err(|e| {
            error!("error selecting devices: {:?}", e);
            Error::Internal
        })
    }

    pub async fn update_device(&self, device_id: &str, update: DeviceUpdate) -> Result<()> {
        let username = &self.username;
        info!("{username} updating device {device_id}: {update:?}");

        let caption: Option<_> = update.caption;
        let type_default = update.r#type.clone().unwrap_or_default();
        let r#type: Option<_> = update.r#type;

        let result = query!(
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
        .execute(&self.sync.0)
        .await;

        match result {
            Ok(_result) => Ok(()),
            Err(e) => {
                error!("error inserting device: {:?}", e);
                Err(Error::Internal)
            }
        }
    }

    pub async fn subscriptions(
        &self,
        device_id: &str,
        since: Timestamp,
    ) -> Result<SubscriptionChangesToClient> {
        let username = &self.username;

        trace!("{username} on {device_id}, requesting subscription changes since {since}");

        #[derive(Debug, sqlx::FromRow)]
        struct Url {
            url: String,
            deleted: Option<Timestamp>,
            created: Timestamp,
        }

        let urls = query_as!(
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
        .fetch_all(&self.sync.0)
        .await
        .map_err(|e| {
            error!("error selecting subscriptions: {e:?}");
            Error::Internal
        })?;

        enum E {
            Created(String),
            Removed(String),
        }

        impl E {
            fn url(self) -> String {
                match self {
                    E::Created(u) => u,
                    E::Removed(u) => u,
                }
            }
            fn is_create(&self) -> bool {
                matches!(self, E::Created(_))
            }
        }

        let latest = urls.iter().map(|u| u.created).max();

        let (created, deleted): (Vec<_>, Vec<_>) = urls
            .into_iter()
            .map(|entry| match entry.deleted {
                Some(_) => E::Removed(entry.url),
                None => E::Created(entry.url),
            })
            .partition(E::is_create);

        let created: Vec<_> = created.into_iter().map(E::url).collect();
        let deleted: Vec<_> = deleted.into_iter().map(E::url).collect();

        info!(
            "{username} on {device_id}, {} subs created, {} deleted",
            created.len(),
            deleted.len(),
        );

        Ok(SubscriptionChangesToClient {
            add: created,
            remove: deleted,
            timestamp: latest.unwrap_or_default(),
        })
    }

    async fn transact<'t, T, R, F>(&self, transaction: T) -> Result<R>
    where
        T: FnOnce(Transaction<'t, Sqlite>) -> F,
        F: Future<Output = Result<(Transaction<'t, Sqlite>, R)>>,
    {
        let tx = self.sync.0.begin().await.map_err(|e| {
            error!("error beginning transaction: {:?}", e);
            Error::Internal
        })?;

        // could probably pass &mut *tx here
        let (tx, r) = transaction(tx).await?;

        tx.commit().await.map_err(|e| {
            error!("error committing transaction: {:?}", e);
            Error::Internal
        })?;

        Ok(r)
    }

    pub async fn update_subscriptions(
        &self,
        device_id: &str,
        changes: SubscriptionChangesFromClient,
    ) -> Result<UpdatedUrls> {
        let username = &self.username;
        let now = now()?;

        trace!("{username} updating subscription for device {device_id}");

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
                    Error::Internal
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
                    Error::Internal
                })?;
            }

            Ok((tx, ()))
        })
        .await?;

        info!(
            "{username} on {device_id}, added {} subscriptions, removed {}",
            changes.add.len(),
            changes.remove.len()
        );

        Ok(UpdatedUrls {
            timestamp: now,
            update_urls: changes
                .add
                .into_iter()
                .map(|url| (url.clone(), url))
                .collect(),
        })
    }

    pub async fn episodes(&self, query: QueryEpisodes) -> Result<Episodes> {
        let username = &self.username;
        let since = query.since.unwrap_or_default();
        let podcast_filter = query.podcast;
        let device_filter = query.device;
        // query.aggregated: unique on (sub, episode)-tuple - always true with how we store

        trace!(
            "{username}, requesting episode changes since {since}, device={}, podcast={}",
            device_filter.as_deref().unwrap_or("<none>"),
            podcast_filter.as_deref().unwrap_or("<none>"),
        );

        let episodes = query_as!(
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
        .fetch_all(&self.sync.0)
        .await
        .map_err(|e| {
            error!("error selecting episodes: {e:?}");
            Error::Internal
        })?;

        let latest = episodes.iter().filter_map(|ep| ep.modified).max();

        let mut episodes = episodes
            .into_iter()
            .map(TryInto::try_into)
            .collect::<result::Result<Vec<Episode>, _>>()
            .map_err(|e| {
                error!("couldn't construct episode changes from DB: {e:?}");
                Error::Internal
            })?;

        // workaround a bug in antennapod - populate the timestamp (EpisodeActionFilter.java:75)
        for ep in &mut episodes {
            if ep.timestamp.is_none() {
                ep.timestamp = Some(Default::default());
            }
        }

        let timestamp = latest.unwrap_or_default();
        info!(
            "{username}, {} episodes changes, latest: {timestamp}",
            episodes.len()
        );

        Ok(Episodes {
            timestamp,
            actions: episodes,
        })
    }

    pub async fn update_episodes(&self, body: Vec<Episode>) -> Result<UpdatedUrls> {
        let username = &self.username;

        trace!("{username} updating episodes");

        let changes = body
            .into_iter()
            .map(TryInto::try_into)
            .collect::<result::Result<Vec<Episode>, _>>()
            .map_err(|e| {
                error!("couldn't construct episode changes from user: {e:?}");
                Error::BadRequest
            })?;

        let now = now()?;
        let change_count = changes.len();

        self.transact(|mut tx| async {
            for change in changes {
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
                            modified = ?
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
                )
                .execute(&mut tx)
                .await
                .map_err(|e| {
                    error!("error querying mid-transaction: {:?}", e);
                    Error::Internal
                })?;
            }

            Ok((tx, ()))
        })
        .await?;

        info!("{username} updated {change_count} episodes");

        Ok(UpdatedUrls::just_timestamp(Timestamp::default()))
    }
}

fn now() -> Result<Timestamp> {
    Timestamp::now().map_err(|()| Error::Internal)
}

impl UpdatedUrls {
    pub fn just_timestamp(timestamp: Timestamp) -> Self {
        Self {
            timestamp,
            update_urls: Default::default(),
        }
    }
}
