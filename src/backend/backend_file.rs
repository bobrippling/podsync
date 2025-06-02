#![allow(unused_variables)]
#![allow(unused_imports)]

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::path::PathBuf;

use log::{error, info, warn};

use crate::backend::FindError;

use crate::device::{DeviceAndSub, DeviceType, DeviceUpdate};
use crate::episode::{Episode, EpisodeRaw};
use crate::podsync::{QueryEpisodes, Url};
use crate::subscription::SubscriptionChangesFromClient;
use crate::user::User;
use crate::Timestamp;

mod kv;
use kv::KeyValues;

pub struct Backend {
    root: PathBuf,
}

// Change introduced to match function signature with backend_sql
pub async fn init(data_dir: &PathBuf) {}

impl Backend {
    pub async fn new(path: &PathBuf) -> Self {
        Self {
            root: path.to_path_buf(),
        }
    }
}

macro_rules! path {
    ($root: expr, $($components: expr),*) => {
        {
            let mut p = $root.clone();
            path!(@internal, p, $($components),*);
            p
        }
    };
    (@internal, $p:expr, $next:expr, $($rest: expr),*) => {
        $p.push($next);
        path!(@internal, $p, $($rest),*);
    };
    (@internal, $p:expr, $next:expr) => {
        $p.push($next);
    };
}

impl Backend {
    fn read(&self, path: PathBuf, keys: &[&str]) -> Result<KeyValues, FindError> {
        let file = File::open(&path).map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                return FindError::NotFound;
            }
            error!("open \"{path:?}\": {e:?}");
            FindError::Internal
        })?;

        kv::read(file, keys)
    }

    fn write(&self, path: PathBuf, keyvalues: &KeyValues) -> Result<(), std::io::Error> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        kv::write(file, keyvalues)
    }

    fn read_user(&self, username: &str) -> Result<KeyValues, FindError> {
        let path = path!(self.root, "users", username, "creds.txt");
        self.read(path, &["pwhash", "session_id"])
    }

    fn write_user(&self, username: &str, keyvalues: &KeyValues) -> Result<(), std::io::Error> {
        let path = path!(self.root, "users", username, "creds.txt");
        self.write(path, keyvalues)
    }
}

impl Backend {
    pub async fn find_user(&self, target_username: &str) -> Result<User, FindError> {
        let user = self.read_user(target_username)?;

        Ok(User {
            username: target_username.into(),
            pwhash: user.get("pwhash").ok_or(FindError::Internal)?.clone(),
            session_id: user.get("session_id").map(|x| x.into()),
        })
    }

    /// session_id: set to None to logout / make NULL
    pub async fn update_user(&self, username: &str, session_id: Option<&str>) -> bool {
        let mut user = match self.read_user(username) {
            Ok(u) => u,
            Err(e) => {
                error!("read \"{username}\": {e:?}");
                return false;
            }
        };

        match session_id {
            Some(id) => {
                user.insert("session_id".into(), id.into());
            }
            None => {
                user.remove("session_id");
            }
        }

        if let Err(e) = self.write_user(username, &user) {
            error!("write \"{username}\": {e:?}");
            false
        } else {
            true
        }
    }

    pub async fn users_with_session(&self, session_id: &str) -> Result<Vec<User>, ()> {
        let path = path!(self.root, "users");
        let mut users = vec![];

        let emap = |e: &dyn std::fmt::Debug| {
            error!("error looking up session: {e:?}");
        };

        for ent in fs::read_dir(path).map_err(|e| emap(&e))? {
            let ent = ent.map_err(|e| emap(&e))?;

            let fname = ent.file_name();
            let fname = match fname.into_string() {
                Ok(x) => x,
                Err(e) => {
                    warn!("couldn't convert path into string: {e:?}");
                    continue;
                }
            };

            let u = self.find_user(&fname).await.map_err(|e| emap(&e))?;
            if let Some(ref id) = u.session_id {
                if id == session_id {
                    users.push(u);
                }
            }
        }

        Ok(users)
    }
}

impl Backend {
    fn devices(&self, username: &str) -> Result<Vec<(String, DeviceType, String)>, ()> {
        let path = path!(self.root, "users", username, "devices.txt");
        let file = File::open(&path).map_err(|e| {
            error!("open \"{path:?}\": {e:?}");
        })?;

        let mut devices = vec![];

        for line in BufReader::new(file).lines() {
            let line = line.map_err(|e| {
                error!("read \"{path:?}\": {e:?}");
            })?;

            let [id, type_, caption] = *line.splitn(3, ' ').collect::<Vec<_>>() else {
                error!("invalid device line");
                return Err(());
            };

            devices.push((
                id.into(),
                type_.try_into().map_err(|()| {
                    error!("couldn't parse device type for \"{username}\"");
                })?,
                caption.into(),
            ));
        }

        Ok(devices)
    }

    pub async fn devices_for_user(&self, username: &str) -> Result<Vec<DeviceAndSub>, ()> {
        let subcount = self.subscriptions_anydev(username)?.len(); // inefficient

        self.devices(username)?
            .into_iter()
            .map(|(id, type_, caption)| {
                Ok(DeviceAndSub {
                    r#type: type_,
                    id,
                    caption,
                    subscriptions: subcount as _,
                })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    pub async fn update_device(
        &self,
        username: &str,
        device_id: &str,
        update: DeviceUpdate,
    ) -> Result<(), ()> {
        let mut devices = self.devices(username)?;
        let mut found = false;

        for dev in &mut devices {
            if dev.0 == device_id {
                if let Some(ref t) = update.r#type {
                    dev.1 = t.clone();
                }
                if let Some(ref c) = update.caption {
                    dev.2 = c.clone();
                }
                found = true;
                break;
            }
        }

        if !found {
            devices.push((
                device_id.into(),
                update.r#type.unwrap_or_default(),
                update.caption.unwrap_or_else(|| "".into()),
            ));
        }

        let path = path!(self.root, "users", username, "devices.txt");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| {
                error!("couldn't open \"{username}\"'s devices: {e:?}");
            })?;

        for dev in devices {
            let (id, type_, caption) = dev;
            writeln!(file, "{id} {} {caption}", type_.as_str()).map_err(|e| {
                error!("writing \"{username}\" devices: {e:?}");
            })?;
        }

        Ok(())
    }
}

impl Backend {
    fn subscriptions_anydev(
        &self,
        username: &str,
    ) -> Result<Vec<(String, String, Timestamp, Option<Timestamp>)>, ()> {
        let path = path!(self.root, "users", username, "subs.txt");
        let file = File::open(&path).map_err(|e| {
            error!("open \"{path:?}\": {e:?}");
        })?;
        let mut subs = vec![];

        for line in BufReader::new(file).lines() {
            let line = line.map_err(|e| {
                error!("read \"{path:?}\": {e:?}");
            })?;

            let [device, created, deleted, url] = *line.splitn(4, ' ').collect::<Vec<_>>() else {
                error!("invalid sub line");
                return Err(());
            };

            let parse = |s: &str| {
                s.parse().map_err(|e| {
                    error!("couldn't parse \"{s}\" as a timestamp: {e:?}");
                })
            };

            subs.push((
                device.into(),
                url.into(),
                parse(created)?,
                match deleted {
                    "-" => None,
                    _ => Some(parse(deleted)?),
                },
            ));
        }

        Ok(subs)
    }

    pub async fn subscriptions(
        &self,
        username: &str,
        device_id: &str,
        since: Timestamp,
    ) -> Result<Vec<Url>, ()> {
        Ok(self
            .subscriptions_anydev(username)?
            .into_iter()
            .filter(|(dev, _url, created, deleted)| {
                if dev != device_id {
                    return false;
                }
                if *created >= since {
                    return true;
                }
                match deleted {
                    Some(deleted) => *deleted >= since,
                    None => false,
                }
            })
            .map(|(dev, url, created, deleted)| Url {
                url,
                created,
                deleted,
            })
            .collect())
    }

    pub async fn update_subscriptions(
        &self,
        username: &str,
        device_id: &str,
        changes: &SubscriptionChangesFromClient,
        now: Timestamp,
    ) -> Result<(), ()> {
        let existing = self.subscriptions_anydev(username)?;
        let urls_to_del = changes.remove.iter().collect::<HashSet<_>>();
        let now = Timestamp::now().map_err(|e| {
            error!("couldn't create timestamp: {e:?}");
        })?;
        let new_subs = changes
            .add
            .iter()
            .map(|url| (device_id, url.as_str(), &now, None));

        let to_write = existing
            .iter()
            .filter(|sub| sub.0 != device_id || !urls_to_del.contains(&sub.1))
            .map(|(dev, url, created, deleted)| {
                (dev.as_str(), url.as_str(), created, deleted.as_ref())
            })
            .chain(new_subs);

        let path = path!(self.root, "users", username, "subs.txt");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| {
                error!("couldn't open \"{username}\"'s subs: {e:?}");
            })?;

        for sub in to_write {
            let (device, url, created, deleted) = sub;

            let r = match deleted {
                Some(deleted) => writeln!(file, "{} {} {} {}", device, created, deleted, url),
                None => writeln!(file, "{} {} - {}", device, created, url),
            };

            r.map_err(|e| {
                error!("writing \"{username}\" subs: {e:?}");
            })?;
        }

        Ok(())
    }
}

impl Backend {
    pub async fn episodes(
        &self,
        username: &str,
        query: &QueryEpisodes,
    ) -> Result<Vec<EpisodeRaw>, ()> {
        let path = path!(self.root, "users", username, "episodes.txt");
        let file = File::open(&path).map_err(|e| {
            error!("open \"{path:?}\": {e:?}");
        })?;

        let mut eps = vec![];

        for line in BufReader::new(file).lines() {
            let line = line.map_err(|e| {
                error!("read \"{path:?}\": {e:?}");
            })?;

            let ep = serde_json::from_str(&line).map_err(|e| {
                error!("couldn't parse episode line for {username}");
            })?;
            eps.push(ep);
        }

        Ok(eps)
    }

    pub async fn update_episodes(
        &self,
        username: &str,
        now: Timestamp,
        changes: Vec<Episode>,
    ) -> Result<(), ()> {
        let mut eps = self.episodes(username, &QueryEpisodes::default()).await?;

        for change in changes {
            // insert `change`, if conflict then replace
            // supplement with username, device, podcast

            let change: EpisodeRaw = change.into();
            let change_id = change.id();
            let found = eps.iter_mut().find(|ep| ep.id() == change_id);

            match found {
                Some(ep) => {
                    *ep = change;
                }
                None => {
                    eps.push(change);
                }
            }
        }

        let path = path!(self.root, "users", username, "episodes.txt");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| {
                error!("couldn't open \"{username}\"'s episodes: {e:?}");
            })?;

        for ep in eps {
            let json = serde_json::to_string(&ep).map_err(|e| {
                error!("couldn't convert episode to json: {e:?}");
            })?;

            writeln!(file, "{}", json).map_err(|e| {
                error!("writing \"{username}\" episode: {e:?}");
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod test {}
