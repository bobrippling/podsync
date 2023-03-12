use std::fmt;

use log::error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
#[derive(sqlx::Type)]
#[sqlx(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    pub fn now() -> Result<Self, ()> {
        use std::time::SystemTime;

        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .map(Self)
            .map_err(|e| {
                error!("couldn't get time: {e:?}");
            })
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Self(0)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            return write!(fmt, "<epoch>");
        }

        use ::time::{format_description::well_known::Rfc3339, OffsetDateTime};

        let formatted = OffsetDateTime::from_unix_timestamp(self.0)
            .ok()
            .and_then(|when| when.format(&Rfc3339).ok());

        match formatted {
            Some(s) => write!(fmt, "{}", s),
            None => write!(fmt, "{}", self.0),
        }
    }
}
