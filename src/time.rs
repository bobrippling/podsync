use std::{fmt, time};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
#[derive(sqlx::Type)]
#[sqlx(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    #[cfg(test)]
    pub fn now() -> Result<Self, time::SystemTimeError> {
        Ok(Self::from_i64(25))
    }

    #[cfg(not(test))]
    pub fn now() -> Result<Self, time::SystemTimeError> {
        use std::time::SystemTime;

        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .map(Self)
    }

    #[cfg(test)]
    pub fn from_i64(i: i64) -> Self {
        Self(i)
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
