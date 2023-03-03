use std::future::Future;

use sqlx::{Pool, Sqlite, Transaction, query, query_as};
use warp::http;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::auth::Credentials;
use crate::user::User;
use crate::split_format_json;
use crate::QuerySince;
use crate::device::{Device, DeviceCreate, DeviceType};
use crate::subscription::SubscriptionChanges;
use crate::episode::{
    EpisodeChanges,
    EpisodeChangeWithDevice,
    EpisodeChangeRaw,
};

pub struct PodSync(Pool<Sqlite>);

pub struct PodSyncAuthed<'s> {
    sync: &'s PodSync,
    user: &'s str,
}

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

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdatedUrls {
    timestamp: u32,
    update_urls: Vec<[String; 2]>,
}

impl PodSync {
    pub fn new(db: Pool<Sqlite>) -> Self {
        Self(db)
    }

    pub async fn authenticate<'s>(
        &'s self, creds: &'s Credentials,
    ) -> Result<PodSyncAuthed<'s>> {
        let username = creds.user();

        let user = query_as!(
                User,
                r#"
                SELECT *
                FROM users
                WHERE username = ?
                "#,
                username,
        )
                .fetch_one(&self.0)
                .await
                .map_err(|e| {
                    if matches!(e, sqlx::Error::RowNotFound) {
                        error!("rejecting non-existant user {}", creds.user());
                    } else {
                        error!("couldn't authenticate user {}: {e:?}", creds.user());
                    }
                    Error::Unauthorized
                })?;

        user.accept_password(creds.pass())
            .then_some(())
            .ok_or_else(|| {
                error!("wrong password for user {}", creds.user());
                Error::Unauthorized
            })
            .map(|()| PodSyncAuthed {
                sync: self,
                user: &creds.user(),
            })
    }
}

impl PodSyncAuthed<'_> {
    pub async fn login(&self) -> Result<()> {
        Ok(())
    }

    pub async fn devices(&self, username_format: String) -> Result<Vec<Device>> {
        let username = split_format_json(&username_format)?;

        // TODO
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

    pub async fn create_device(
        &self,
        device_name: String,
        new_device: DeviceCreate
    ) -> Result<()>
    {
        let username = self.user;
        println!("got device creation {device_name} for {username}: {new_device:?}");

        let caption = new_device.caption.as_deref().unwrap_or("");
        let r#type = new_device.r#type.unwrap_or(DeviceType::Other);

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
        device_id: String,
        changes: SubscriptionChanges
    ) -> Result<UpdatedUrls> {
        let username = self.user;

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
        println!("episodes GET for {} since {query:?}", self.user);

        // TODO
        // let result = query_as!(
        //     EpisodeChangeRaw,
        //     r#"
        //     SELECT podcast, episode, timestamp, guid, action, started, position, total
        //     FROM episodes
        //     WHERE username = ?
        //     "#,
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
        println!("episodes POST for {}", self.user);

        let username = self.user;

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
