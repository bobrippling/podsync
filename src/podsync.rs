use std::str::FromStr;
use std::{
    future::Future,
    sync::Arc
};

use sqlx::{Pool, Sqlite, Transaction, query, query_as};
use warp::http;
use serde::Serialize;
use log::{error, trace};

use crate::auth::{AuthAttempt, SessionId};
use crate::user::User;
use crate::QuerySince;
use crate::device::{DeviceAndSub, DeviceUpdate};
use crate::subscription::{SubscriptionChangesToClient, SubscriptionChangesFromClient};
use crate::episode::{
    EpisodeChanges,
    EpisodeChangeWithDevice,
    EpisodeChangeRaw,
};
use crate::time::Timestamp;

pub struct PodSync(Pool<Sqlite>);

pub struct PodSyncAuthed<const USER_MATCH: bool = false> {
    sync: Arc<PodSync>,
    session_id: SessionId,
    username: String,
}

#[derive(Debug, Serialize)]
pub struct UpdatedUrls {
    timestamp: u64,
    update_urls: Vec<(String, String)>,
}

#[derive(Debug)]
pub enum Error {
    Internal,
    Unauthorized,
    BadRequest,
}

pub type Result<T> = std::result::Result<T, Error>;

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

        let ok = |session_id| Ok(PodSyncAuthed {
            sync: Arc::clone(self),
            session_id,
            username: auth_attempt.user().to_string(),
        });

        let db_session_id = match user.session_id {
            Some(ref id) => {
                 let session_id = SessionId::from_str(&id)
                     .map_err(|()| {
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

                trace!("{username} login: new session created");
                ok(session_id)
            }
            (Some(client), Some(db_id)) => {
                if client == db_id {
                    trace!("{username} login: session check passed");
                    ok(client)
                } else {
                    trace!("{username} login: session check failed");
                    Err(Error::Internal)
                }
            }
            (Some(_), None) => {
                // logged out but somehow kept their token?
                trace!("{username} login: no session in db");
                Err(Error::Unauthorized)
            }
            (None, Some(db_id)) => {
                // logging in again, client's forgot their token
                trace!("{username} login: fresh login");
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
            },
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
        trace!("{username} logout");

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
            .map_err(|e| {
                error!("error selecting devices: {:?}", e);
                Error::Internal
            })
    }

    pub async fn update_device(
        &self,
        device_id: &str,
        update: DeviceUpdate
    ) -> Result<()>
    {
        let username = &self.username;
        trace!("{username} updating device {device_id}: {update:?}");

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
            device_id, username, caption, type_default,
            caption, r#type,
            device_id, username
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

    pub async fn subscriptions(&self, device_id: &str, since: Timestamp)
        -> Result<SubscriptionChangesToClient>
    {
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
            .map(|entry| {
                match entry.deleted {
                    Some(_) => E::Removed(entry.url),
                    None => E::Created(entry.url),
                }
            })
            .partition(E::is_create);

        let created = created.into_iter().map(E::url).collect();
        let deleted = deleted.into_iter().map(E::url).collect();

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
        let tx = self.sync.0
            .begin()
            .await
            .map_err(|e| {
                error!("error beginning transaction: {:?}", e);
                Error::Internal
            })?;

        // could probably pass &mut *tx here
        let (tx, r) = transaction(tx).await?;

        tx.commit()
            .await
            .map_err(|e| {
                error!("error committing transaction: {:?}", e);
                Error::Internal
            })?;

        Ok(r)
    }

    pub async fn update_subscriptions(
        &self,
        device_id: &str,
        changes: SubscriptionChangesFromClient
    ) -> Result<UpdatedUrls> {
        use std::time::SystemTime;

        let username = &self.username;

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| {
                error!("couldn't get time: {e:?}");
                Error::Internal
            })?
            .as_secs();
        let now = timestamp as i64;

        trace!("subscription update for {username}'s device {device_id}");

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
        }).await?;

        Ok(UpdatedUrls {
            // important: this timestamp is used by future client synchronisations
            timestamp,
            // unused by client:
            update_urls: changes.add
                .into_iter()
                .map(|url| (url.clone(), url))
                .collect(),
        })
    }

    pub async fn episodes(&self, query: QuerySince)
        -> Result<EpisodeChanges>
    {
        // podcast, episode, action, timestamp?, guid?, ...{started,position,total}?
        println!("episodes GET for {} since {query:?}", self.username);

        // TODO
        // let result = query_as!(
        //     EpisodeChangeRaw,
        //     "
        //     SELECT podcast, episode, timestamp, guid, action, started, position, total
        //     FROM episodes
        //     WHERE username = ?
        //     ",
        //     username,
        // )
        //     .fetch_all(&*db)
        //     .await;

        Ok(EpisodeChanges::empty_at(0))
    }

    pub async fn update_episodes(&self, body: Vec<EpisodeChangeWithDevice>)
        -> Result<UpdatedUrls>
    {
        // podcast, episode, guid: optional
        println!("episodes POST for {}", self.username);

        let username = &self.username;

        self.transact(|mut tx| async move {
            for EpisodeChangeWithDevice { change, device } in body {
                println!("updating: {change:?} device={device}");

                let EpisodeChangeRaw {
                    podcast, episode, timestamp, guid,
                    action, started, position, total,
                } = change.into();

                query!(
                    "
                    INSERT INTO episodes
                    (
                        username, device, podcast,
                        episode, timestamp, guid,
                        action,
                        started, position, total
                    )
                    VALUES
                    (
                        ?, ?, ?,
                        ?, ?, ?,
                        ?,
                        ?, ?, ?
                    )
                    ON CONFLICT (username, device, podcast, episode)
                    DO
                        UPDATE SET
                            timestamp = ?,
                            guid = ?,
                            action = ?,
                            started = ?,
                            position = ?,
                            total = ?
                    ",
                    username, device, podcast,
                    episode, timestamp, guid,
                    action,
                    started, position, total,
                    //
                    timestamp, guid,
                    action,
                    started, position, total
                )
                    .execute(&mut tx)
                    .await
                    .map_err(|e| {
                        error!("error querying mid-transaction: {:?}", e);
                        Error::Internal
                    })?;
            }

            Ok((tx, ()))
        }).await?;

        Ok(UpdatedUrls { // FIXME: rename struct
            timestamp: 0, // FIXME: timestamping
            update_urls: vec![], // unused by client
        })
    }
}
