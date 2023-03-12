use std::fmt;

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
#[derive(sqlx::Type)]
#[sqlx(transparent)]
pub struct Timestamp(i64);

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

        use ::time::{OffsetDateTime, format_description::well_known::Rfc3339};

        let formatted = OffsetDateTime::from_unix_timestamp(self.0)
            .ok()
            .and_then(|when| when.format(&Rfc3339).ok());

        match formatted {
            Some(s) => write!(fmt, "{}", s),
            None => write!(fmt, "{}", self.0)
        }
    }
}
