use serde::{Deserialize, Serialize};
use time::{
    macros::{date, time},
    PrimitiveDateTime,
};

// this struct exists to work around #[serde(with = ...)]
// not handling Option for us
#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
#[serde(transparent)]
#[cfg_attr(feature = "backend-sql", derive(sqlx::Type), sqlx(transparent))]
pub struct Time(#[serde(with = "time_no_offset")] PrimitiveDateTime);

impl Time {
    pub fn epoch() -> Self {
        let dt = PrimitiveDateTime::new(date!(1970 - 01 - 01), time!(0:00));
        Self(dt)
    }
}

impl Time {
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn from_i64(i: i64) -> Self {
        use time::Time;
        let dt = PrimitiveDateTime::new(
            date!(1970 - 01 - 01),
            Time::from_hms(0, 0, i.try_into().unwrap()).unwrap(),
        );
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
