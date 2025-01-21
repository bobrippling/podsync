use serde::{Deserialize, Serialize};

pub type TimePrimitive = i64;

#[derive(Debug, Clone, Hash)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub enum EpisodeAction {
    New,
    Download,
    Play {
        started: TimePrimitive,
        position: TimePrimitive,
        total: TimePrimitive,
    },
    Delete,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "backend-sql", derive(sqlx::Type))]
#[serde(rename_all = "lowercase")]
pub enum EpisodeActionRaw {
    New,
    Download,
    Play,
    Delete,
}

impl From<EpisodeAction>
    for (
        EpisodeActionRaw,
        Option<TimePrimitive>,
        Option<TimePrimitive>,
        Option<TimePrimitive>,
    )
{
    fn from(
        episode: EpisodeAction,
    ) -> (
        EpisodeActionRaw,
        Option<TimePrimitive>,
        Option<TimePrimitive>,
        Option<TimePrimitive>,
    ) {
        match episode {
            EpisodeAction::New => (EpisodeActionRaw::New, None, None, None),
            EpisodeAction::Download => (EpisodeActionRaw::Download, None, None, None),
            EpisodeAction::Delete => (EpisodeActionRaw::Delete, None, None, None),
            EpisodeAction::Play {
                started,
                position,
                total,
            } => (
                EpisodeActionRaw::Play,
                Some(started),
                Some(position),
                Some(total),
            ),
        }
    }
}

impl
    TryFrom<(
        EpisodeActionRaw,
        Option<TimePrimitive>,
        Option<TimePrimitive>,
        Option<TimePrimitive>,
    )> for EpisodeAction
{
    type Error = &'static str;

    fn try_from(
        tup: (
            EpisodeActionRaw,
            Option<TimePrimitive>,
            Option<TimePrimitive>,
            Option<TimePrimitive>,
        ),
    ) -> Result<EpisodeAction, &'static str> {
        Ok(match tup {
            (EpisodeActionRaw::New, _, _, _) => Self::New,
            (EpisodeActionRaw::Download, _, _, _) => Self::Download,
            (EpisodeActionRaw::Delete, _, _, _) => Self::Delete,
            (EpisodeActionRaw::Play, Some(started), Some(position), Some(total)) => Self::Play {
                started,
                position,
                total,
            },
            _ => return Err("\"play\" without started/position/total"),
        })
    }
}
