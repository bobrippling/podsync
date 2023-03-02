use std::future::Future;

use sqlx::{Pool, Sqlite, Transaction, query, query_as};
use warp::http::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::QuerySince;
use crate::device::{Device, DeviceCreate, DeviceType};
use crate::subscription::SubscriptionChanges;
use crate::episode::{
    EpisodeChanges,
    EpisodeChangeWithDevice,
    EpisodeChangeRaw,
};

pub struct PodSync(Pool<Sqlite>);

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdatedUrls {
    timestamp: u32,
    update_urls: Vec<[String; 2]>,
}

impl PodSync {
    pub fn new(db: Pool<Sqlite>) -> Self {
        Self(db)
    }

    pub async fn login(&self, username: String, auth: String)
        -> Result<(), StatusCode>
    {
        eprintln!("todo: auth or {username}: {auth}");

        Ok(())
    }

    pub async fn devices(&self, username_format: String)
        -> Result<Vec<Device>, StatusCode>
    {
        let (username, format) = split_dot(&username_format)?;
        err_unless_json(format)?;

        let result = query_as!(
                Device,
                r#"
                SELECT id, caption, type as "type: _", subscriptions, username
                FROM devices
                WHERE username = ?
                "#,
                username,
        )
                .fetch_all(&self.0)
                .await;

        match result {
            Ok(devices) => Ok(devices),
            Err(e) => {
                error!("error selecting devices: {:?}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    pub async fn create_device(
        &self,
        username: String,
        device_name: String,
        new_device: DeviceCreate
    ) -> Result<(), StatusCode>
    {
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
            .execute(&self.0)
            .await;

        match result {
            Ok(_result) => Ok(()),
            Err(e) => {
                // FIXME: handle EEXIST (and others?)
                error!("error inserting device: {:?}", e);

                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    pub async fn subscriptions(&self, _username: String, _deviceid_format: String)
        -> Result<SubscriptionChanges, StatusCode>
    {
        // println!("got subscriptions for {deviceid_format} for {username}");

        Ok(SubscriptionChanges::empty())
    }

    async fn transact<'t, T, R, F>(&self, transaction: T) -> Result<R, StatusCode>
    where
        T: FnOnce(Transaction<'t, Sqlite>) -> F,
        F: Future<Output = Result<(Transaction<'t, Sqlite>, R), StatusCode>>,
    {
        let tx = self.0
            .begin()
            .await
            .map_err(|e| {
                error!("error beginning transaction: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let (tx, r) = transaction(tx).await?;

        tx.commit()
            .await
            .map_err(|e| {
                error!("error committing transaction: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        Ok(r)
    }

    pub async fn update_subscriptions(
        &self,
        username: String,
        deviceid_format: String,
        changes: SubscriptionChanges
    ) -> Result<UpdatedUrls, StatusCode> {
        let (device_id, format) = split_dot(&deviceid_format)?;
        err_unless_json(format)?;

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
                        StatusCode::INTERNAL_SERVER_ERROR
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
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
            }

            Ok((tx, ()))
        }).await?;

        Ok(UpdatedUrls {
            timestamp: 0, // TODO
            update_urls: vec![], // unused by client
        })
    }

    pub async fn episodes(&self, username_format: String, query: QuerySince)
        -> Result<EpisodeChanges, StatusCode>
    {
        let (_username, format) = split_dot(&username_format)?;
        err_unless_json(format)?;

        // podcast, episode, action, timestamp?, guid?, ...{started,position,total}?
        println!("episodes GET for {username_format} since {query:?}");

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

    pub async fn update_episodes(&self, username_format: String, body: Vec<EpisodeChangeWithDevice>)
        -> Result<UpdatedUrls, StatusCode>
    {
        // podcast, episode, guid: optional
        println!("episodes POST for {username_format}");

        let (username, format) = split_dot(&username_format)?;
        err_unless_json(format)?;

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
                        StatusCode::INTERNAL_SERVER_ERROR
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

fn split_dot(s: &str) -> Result<(&str, &str), StatusCode> {
    s.split_once('.').ok_or(StatusCode::BAD_REQUEST)
}

fn err_unless_json(s: &str) -> Result<(), StatusCode> {
    (s == "json")
        .then_some(())
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)
}
