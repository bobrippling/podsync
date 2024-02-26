use std::fs::File;
use std::future::Future;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use log::{error, info};

use crate::backend::FindError;

use crate::device::{DeviceAndSub, DeviceUpdate};
use crate::episode::{Episode, EpisodeRaw};
use crate::podsync::{QueryEpisodes, Url};
use crate::subscription::SubscriptionChangesFromClient;
use crate::user::User;
use crate::Timestamp;

type Result<T> = std::result::Result<T, ()>;

pub struct Backend {
    root: PathBuf,
}

pub async fn init() {}

impl Backend {
    pub async fn new() -> Self {
        Self {
            root: PathBuf::from("./"),
        }
    }
}

impl Backend {
    fn open(&self, filename: &str) -> Result<File> {
        let mut p = self.root.clone();
        p.push(filename);

        File::open(&p).map_err(|e| {
            error!("couldn't open {:?}", p);
        })
    }
}

impl Backend {
    pub async fn find_user(&self, target_username: &str) -> std::result::Result<User, FindError> {
        let file = self.open("users.txt").map_err(|()| FindError::Internal)?;

        for line in BufReader::new(file).lines() {
            let line = line.map_err(|e| {
                error!("couldn't read line: {e}");
                FindError::Internal
            })?;

            let parts = line.split(' ').collect::<Vec<_>>();
            let [username, pwhash, session_id] = parts[..] else {
                error!("invalid users line format");
                return Err(FindError::Internal);
            };

            if username == target_username {
                return Ok(User {
                    username: username.into(),
                    pwhash: pwhash.into(),
                    session_id: if session_id.len() > 0 {
                        Some(session_id.into())
                    } else {
                        None
                    },
                });
            }
        }

        Err(FindError::NotFound)
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

    fn test_find_user() {}
}
