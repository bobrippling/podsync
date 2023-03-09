use std::str::FromStr;
use std::{
    future::Future,
    sync::Arc
};

use sqlx::{Pool, Sqlite, Transaction, query, query_as};
use warp::http;
use serde::{Deserialize, Serialize};
use log::{error, trace};

use crate::auth::{AuthAttempt, SessionId};
use crate::user::User;
use crate::QuerySince;
use crate::device::{Device, DeviceUpdate, DeviceType};
use crate::subscription::SubscriptionChanges;
use crate::episode::{
    EpisodeChanges,
    EpisodeChangeWithDevice,
    EpisodeChangeRaw,
};

pub struct PodSync(Pool<Sqlite>);

pub struct PodSyncAuthed<const USER_MATCH: bool = false> {
    sync: Arc<PodSync>,
    session_id: SessionId,
    username: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdatedUrls {
    timestamp: u32,
    update_urls: Vec<[String; 2]>,
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

                trace!("new session created");
                ok(session_id)
            }
            (Some(client), Some(db_id)) => {
                if client == db_id {
                    trace!("session check passed");
                    ok(client)
                } else {
                    trace!("session check failed");
                    Err(Error::Internal)
                }
            }
            (Some(_), None) => {
                // logged out but somehow kept their token?
                trace!("no session in db");
                Err(Error::Unauthorized)
            }
            (None, Some(db_id)) => {
                // logging in again, client's forgot their token
                trace!("fresh login");
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

    pub async fn devices(&self) -> Result<Vec<Device>> {
        let username = &self.username;

        let result = query_as!(
                Device,
                r#"
                SELECT caption, type as "type: _", subscriptions, username
                FROM devices
                WHERE username = ?
                "#,
                username,
        )
                .fetch_all(&self.sync.0)
                .await;

        match result {
            Ok(devices) => Ok(devices),
            Err(e) => {
                error!("error selecting devices: {:?}", e);
                Err(Error::Internal)
            }
        }
    }

    pub async fn update_device(
        &self,
        device_name: String,
        new_device: DeviceUpdate
    ) -> Result<()>
    {
        let username = &self.username;
        trace!("{username} updating device {device_name}: {new_device:?}");

        let caption = new_device.caption.as_deref().unwrap_or("");
        let r#type = new_device.r#type.unwrap_or(DeviceType::Other);

        // FIXME: update only values that've been provided

        let result = query!(
            "INSERT INTO devices
            (caption, type, username, subscriptions)
            VALUES
            (?, ?, ?, ?)",
            caption,
            r#type,
            username,
            0,
        )
            .execute(&self.sync.0)
            .await;

        match result {
            Ok(_result) => Ok(()),
            Err(e) => {
                // FIXME: handle EEXIST (and others?)
                error!("error inserting device: {:?}", e);

                Err(Error::Internal)
            }
        }
    }

    pub async fn subscriptions(&self, _device_id: &str)
        -> Result<SubscriptionChanges>
    {
        // println!("got subscriptions for {device_id} for {username}");

        Ok(SubscriptionChanges::empty())
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
        changes: SubscriptionChanges
    ) -> Result<UpdatedUrls> {
        let username = &self.username;

        println!("got urls for {username}'s device {device_id}, timestamp {:?}:", changes.timestamp);

        self.transact(move |mut tx| async move {
            for url in &changes.remove {
                query!(
                    "
                    DELETE FROM subscriptions
                    WHERE username = ?
                    AND device = ?
                    AND url = ?
                    ",
                    username,
                    device_id,
                    url,
                )
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| {
                        error!("error querying mid-transaction: {:?}", e);
                        Error::Internal
                    })?;
            }

            for url in &changes.add {
                query!(
                    "
                    INSERT INTO subscriptions
                    (url, device, username)
                    VALUES
                    (?, ?, ?)
                    ON CONFLICT (url, device, username)
                    DO NOTHING
                    ",
                    url,
                    device_id,
                    username,
                )
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| {
                        error!("error querying mid-transaction: {:?}", e);
                        Error::Internal
                    })?;
            }

            Ok((tx, ()))
        }).await?;

        Ok(UpdatedUrls {
            timestamp: 0, // TODO
            update_urls: vec![], // unused by client
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
