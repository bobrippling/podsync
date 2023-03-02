use serde::{Deserialize, Serialize};

time::serde::format_description!(
    time_no_offset,
    PrimitiveDateTime,
    "[year]-[month]-[day]T[hour]:[minute]:[second]" // yyyy-MM-dd'T'HH:mm:ss
);

#[derive(Debug, Deserialize, Serialize)]
pub struct EpisodeChanges {
    timestamp: u32,
    actions: Vec<EpisodeChange>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(try_from = "EpisodeChangeRaw", into = "EpisodeChangeRaw")]
pub struct EpisodeChange {
    podcast: String,
    episode: String,
    timestamp: time::PrimitiveDateTime,
    guid: Option<String>,
    action: EpisodeAction,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EpisodeChangeWithDevice {
    #[serde(flatten)]
    pub change: EpisodeChange,
    pub device: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum EpisodeAction {
    New,
    Download,
    Play {
        started: u32,
        position: u32,
        total: u32,
    },
    Delete,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EpisodeChangeRaw {
    pub podcast: String,
    pub episode: String,
    #[serde(with = "time_no_offset")]
    pub timestamp: time::PrimitiveDateTime,
    pub guid: Option<String>,
    pub action: EpisodeActionRaw,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[derive(sqlx::Type)]
#[serde(rename_all = "lowercase")]
pub enum EpisodeActionRaw {
    New,
    Download,
    Play,
    Delete,
}

impl EpisodeChanges {
    pub fn empty_at(timestamp: u32) -> Self {
        Self {
            timestamp,
            actions: vec![],
        }
    }
}

impl TryFrom<EpisodeChangeRaw> for EpisodeChange {
    type Error = &'static str;

    fn try_from(raw: EpisodeChangeRaw) -> Result<Self, <Self as TryFrom<EpisodeChangeRaw>>::Error> {
        let EpisodeChangeRaw {
            podcast,
            episode,
            timestamp,
            guid,
            action,
            started,
            position,
            total,
        } = raw;

        let action = (action, started, position, total).try_into()?;

        Ok(Self {
            podcast,
            episode,
            timestamp,
            guid,
            action,
        })
    }
}

impl From<EpisodeChange> for EpisodeChangeRaw {
    fn from(episode: EpisodeChange) -> EpisodeChangeRaw {
        let EpisodeChange {
            podcast,
            episode,
            timestamp,
            guid,
            action,
        } = episode;

        let (action, started, position, total) = action.into();

        Self {
            podcast,
            episode,
            timestamp,
            guid,
            action,
            started,
            position,
            total,
        }
    }
}

impl From<EpisodeAction> for (EpisodeActionRaw, Option<u32>, Option<u32>, Option<u32>) {
    fn from(episode: EpisodeAction) -> (EpisodeActionRaw, Option<u32>, Option<u32>, Option<u32>) {
        match episode {
            EpisodeAction::New => (EpisodeActionRaw::New, None, None, None),
            EpisodeAction::Download => (EpisodeActionRaw::Download, None, None, None),
            EpisodeAction::Delete => (EpisodeActionRaw::Delete, None, None, None),
            EpisodeAction::Play { started, position, total } => (
                EpisodeActionRaw::Play,
                Some(started),
                Some(position),
                Some(total),
            ),
        }
    }
}

impl TryFrom<(EpisodeActionRaw, Option<u32>, Option<u32>, Option<u32>)> for EpisodeAction {
    type Error = &'static str;

    fn try_from(tup: (EpisodeActionRaw, Option<u32>, Option<u32>, Option<u32>)) -> Result<EpisodeAction, &'static str> {
        Ok(match tup {
            (EpisodeActionRaw::New, _, _, _) => Self::New,
            (EpisodeActionRaw::Download, _, _, _) => Self::Download,
            (EpisodeActionRaw::Delete, _, _, _) => Self::Delete,
            (EpisodeActionRaw::Play, Some(started), Some(position), Some(total)) =>
                Self::Play {
                    started,
                    position,
                    total,
                },
            _ => return Err("\"play\" without started/position/total"),
        })
    }
}
