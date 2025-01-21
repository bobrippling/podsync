use serde::{Deserialize, Serialize};

use super::{action::TimePrimitive, time::Time, EpisodeAction, EpisodeActionRaw};
use crate::time::Timestamp;

#[derive(Debug, Clone, Hash)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde_with::skip_serializing_none]
#[derive(Deserialize, Serialize)]
#[serde(try_from = "EpisodeRaw", into = "EpisodeRaw")]
pub struct Episode {
    pub podcast: String,
    pub episode: String,
    pub timestamp: Option<Time>,
    pub guid: Option<String>,
    pub action: EpisodeAction,
    pub device: Option<String>, // optional on from-client, not present on to-client
}

impl Episode {
    #[allow(dead_code)]
    pub fn hash(&self) -> String {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        let mut hasher = DefaultHasher::new();
        Hash::hash(self, &mut hasher);
        hasher.finish().to_string()
    }
}

#[derive(Debug)]
#[serde_with::skip_serializing_none]
#[derive(Deserialize, Serialize)] // transitive, from Episode
#[cfg_attr(feature = "backend-sql", derive(sqlx::Type))]
pub struct EpisodeRaw {
    pub device: Option<String>,
    pub podcast: String,
    pub episode: String,

    pub timestamp: Option<Time>,
    pub guid: Option<String>,
    pub action: EpisodeActionRaw,
    pub started: Option<TimePrimitive>,
    pub position: Option<TimePrimitive>,
    pub total: Option<TimePrimitive>,

    pub modified: Option<Timestamp>, // for db, not for http
}

// episodes are unique on username, podcast & episode
// (we assume username is dealt with elsewhere)
#[derive(PartialEq, Eq)]
pub struct EpisodeId<'e> {
    podcast: &'e str,
    episode: &'e str,
    timestamp: Option<&'e Time>,
}

impl EpisodeRaw {
    fn from_episode(ep: Episode, modified: Option<Timestamp>) -> Self {
        let Episode {
            podcast,
            episode,
            timestamp,
            guid,
            action,
            device,
        } = ep;

        let (action, started, position, total) = action.into();

        Self {
            podcast,
            episode,
            timestamp: timestamp.map(Into::into),
            guid,
            action,
            started,
            position,
            total,
            device,
            modified,
        }
    }

    #[allow(dead_code)]
    pub fn id(&self) -> EpisodeId<'_> {
        EpisodeId {
            podcast: self.podcast.as_str(),
            episode: self.episode.as_str(),
            timestamp: self.timestamp.as_ref(),
        }
    }
}

impl TryFrom<EpisodeRaw> for Episode {
    type Error = &'static str;

    fn try_from(raw: EpisodeRaw) -> Result<Self, <Self as TryFrom<EpisodeRaw>>::Error> {
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
        } = raw;

        let action = (action, started, position, total).try_into()?;

        Ok(Self {
            podcast,
            episode,
            timestamp: timestamp.map(Into::into),
            guid,
            action,
            device,
        })
    }
}

impl From<(Episode, Timestamp)> for EpisodeRaw {
    fn from((episode, modified): (Episode, Timestamp)) -> EpisodeRaw {
        Self::from_episode(episode, Some(modified))
    }
}

impl From<Episode> for EpisodeRaw {
    fn from(episode: Episode) -> EpisodeRaw {
        Self::from_episode(episode, None)
    }
}
