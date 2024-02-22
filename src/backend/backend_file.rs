use std::future::Future;

use log::{error, info};

use crate::backend::FindError;

use crate::device::{DeviceAndSub, DeviceUpdate};
use crate::episode::{Episode, EpisodeRaw};
use crate::podsync::{QueryEpisodes, Url};
use crate::subscription::SubscriptionChangesFromClient;
use crate::user::User;
use crate::Timestamp;

type Result<T> = std::result::Result<T, ()>;

pub struct Backend;

pub async fn init() {}

impl Backend {
    pub async fn new() -> Self {
        Self
    }
}

impl Backend {
    pub async fn find_user(&self, username: &str) -> std::result::Result<User, FindError> {
        todo!()
    }

    /// session_id: set to None to logout / make NULL
    pub async fn update_user(&self, username: &str, session_id: Option<&str>) -> bool {
        todo!()
    }

    pub async fn users_with_session(&self, session_id: &str) -> Result<Vec<User>> {
        todo!()
    }
}

impl Backend {
    pub async fn devices_for_user(&self, username: &str) -> Result<Vec<DeviceAndSub>> {
        todo!()
    }

    pub async fn update_device(
        &self,
        username: &str,
        device_id: &str,
        update: DeviceUpdate,
    ) -> Result<()> {
        todo!()
    }
}

impl Backend {
    pub async fn subscriptions(
        &self,
        username: &str,
        device_id: &str,
        since: Timestamp,
    ) -> Result<Vec<Url>> {
        todo!()
    }

    pub async fn update_subscriptions(
        &self,
        username: &str,
        device_id: &str,
        changes: &SubscriptionChangesFromClient,
        now: Timestamp,
    ) -> Result<()> {
        todo!()
    }
}

impl Backend {
    pub async fn episodes(&self, username: &str, query: &QueryEpisodes) -> Result<Vec<EpisodeRaw>> {
        todo!()
    }

    pub async fn update_episodes(
        &self,
        username: &str,
        now: Timestamp,
        changes: Vec<Episode>,
    ) -> Result<()> {
        todo!()
    }
}

#[cfg(test)]
pub mod test {
    use sqlx::{migrate::MigrateDatabase, Pool, Sqlite, SqlitePool};

    pub async fn create_db() -> Pool<Sqlite> {
        todo!()
    }
}
