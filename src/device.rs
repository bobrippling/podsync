use serde::{Deserialize, Serialize};

use sqlx::FromRow;

#[derive(Debug, Serialize, FromRow)]
pub struct Device {
    // pub id: i64, // FIXME: String, convert when pulling out of the DB? change the DB type?
    pub caption: String,

    // #[sqlx(try_from = "String")]
    pub r#type: DeviceType,

    pub subscriptions: i64,

    #[serde(skip)]
    pub username: String,
}

#[derive(Debug, Deserialize, Serialize)] // FIXME: drop Serialize
pub struct DeviceCreate { // FIXME: allow "" to deserialise to this
    pub caption: Option<String>,
    pub r#type: Option<DeviceType>,
}

#[derive(Debug, Deserialize, Serialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Desktop,
    Laptop,
    Mobile,
    Server,
    // #[serde(with = "null_type")] TODO
    Other, // aka null
}

// mod null_type {
//     deserialize
//     serialize
// }

impl TryFrom<&'_ str> for DeviceType {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "Mobile" => Ok(DeviceType::Mobile),
            _ => Err(())
        }
    }
}

impl TryFrom<String> for DeviceType {
    type Error = ();

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_from(&*s)
    }
}
