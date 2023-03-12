use serde::{Serialize, Deserialize};

#[derive(Debug, sqlx::Type, Serialize)]
pub struct DeviceAndSub {
    pub id: String,
    pub caption: String,
    pub r#type: DeviceType,
    pub subscriptions: u32,
}

#[derive(Debug, Deserialize)]
pub struct DeviceUpdate {
    pub caption: Option<String>,
    pub r#type: Option<DeviceType>,
}

#[derive(Debug, Deserialize, Serialize, sqlx::Type, Clone)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Desktop,
    Laptop,
    Mobile,
    Server,
    Other, // aka null
}

impl Default for DeviceType {
    fn default() -> Self { Self::Other }
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
