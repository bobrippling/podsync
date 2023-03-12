use serde::{Deserialize, Serialize};
use time::{
    macros::{date, time},
    PrimitiveDateTime,
};

// this struct exists to work around #[serde(with = ...)]
// not handling Option for us
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(transparent)]
#[derive(sqlx::Type)]
#[sqlx(transparent)]
pub struct Time(#[serde(with = "time_no_offset")] PrimitiveDateTime);

impl Default for Time {
    fn default() -> Self {
        let dt = PrimitiveDateTime::new(date!(1970 - 01 - 01), time!(0:00));
        Self(dt)
    }
}

impl Into<PrimitiveDateTime> for Time {
    fn into(self) -> PrimitiveDateTime {
        self.0
    }
}

impl From<PrimitiveDateTime> for Time {
    fn from(dt: PrimitiveDateTime) -> Self {
        Self(dt)
    }
}

time::serde::format_description!(
    time_no_offset,
    PrimitiveDateTime,
    "[year]-[month]-[day]T[hour]:[minute]:[second]" // yyyy-MM-dd'T'HH:mm:ss
);
