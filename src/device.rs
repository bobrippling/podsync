use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "backend-sql", derive(sqlx::Type))]
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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[cfg_attr(feature = "backend-sql", derive(sqlx::Type))]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Desktop,
    Laptop,
    Mobile,
    Server,
    Other, // aka null
}

impl DeviceType {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Desktop => "Desktop",
            Self::Laptop => "Laptop",
            Self::Mobile => "Mobile",
            Self::Server => "Server",
            Self::Other => "Other",
        }
    }
}

impl Default for DeviceType {
    fn default() -> Self {
        Self::Other
    }
}

impl TryFrom<&'_ str> for DeviceType {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "Mobile" => Ok(DeviceType::Mobile),
            _ => Err(()),
        }
    }
}

impl TryFrom<String> for DeviceType {
    type Error = ();

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_from(&*s)
    }
}
