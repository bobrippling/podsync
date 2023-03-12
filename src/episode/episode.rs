use serde::{Deserialize, Serialize};

use super::{EpisodeAction, EpisodeActionRaw};
use super::action::TimePrimitive;
use crate::time::Timestamp;

#[derive(Debug, Clone)]
#[serde_with::skip_serializing_none]
#[derive(Deserialize, Serialize)]
#[serde(try_from = "EpisodeRaw", into = "EpisodeRaw")]
pub struct Episode {
    pub podcast: String,
    pub episode: String,
    pub timestamp: Option<Timestamp>,
    pub guid: Option<String>,
    pub action: EpisodeAction,
    pub device: Option<String>, // optional on from-client, not present on to-client
}

#[derive(Debug)]
#[serde_with::skip_serializing_none]
#[derive(Deserialize, Serialize)] // transitive, from Episode
#[derive(sqlx::Type)]
pub struct EpisodeRaw {
    pub device: Option<String>,
    pub podcast: String,
    pub episode: String,

    pub timestamp: Option<Timestamp>,
    pub guid: Option<String>,
    pub action: EpisodeActionRaw,
    pub started: Option<TimePrimitive>,
    pub position: Option<TimePrimitive>,
    pub total: Option<TimePrimitive>,

    pub modified: Option<Timestamp>, // for db, not for http
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
