use std::str::FromStr;

use base64_light::base64_decode;
use log::error;
use sha256::digest;
use uuid::Uuid;

use crate::podsync;

pub struct BasicAuth {
    user: String,
    pass: String,
}

pub struct AuthAttempt {
    auth: BasicAuth,
}

#[derive(PartialEq, Eq)]
pub struct SessionId(Uuid);

pub fn pwhash(s: &str) -> String {
    digest(s)
}

impl BasicAuth {
    pub fn with_path_username(self, username: &str) -> podsync::Result<AuthAttempt> {
        (self.user == username)
            .then(|| AuthAttempt { auth: self })
            .ok_or(podsync::Error::Unauthorized)
    }
}

impl FromStr for BasicAuth {
    type Err = &'static str;

    fn from_str(header: &str) -> Result<Self, Self::Err> {
        let (basic, auth_b64) = header.split_once(' ').ok_or("no space in auth header")?;

        if basic != "Basic" {
            return Err("only basic auth supported");
        }

        let auth_bytes = base64_decode(auth_b64);
        let auth = std::str::from_utf8(&auth_bytes).map_err(|e| {
            error!("invalid utf-8 for password: {e:?}");
            "none-utf8 in auth header"
        })?;

        let (user, pass) = auth.split_once(':').ok_or("no colon in auth value")?;

        let user = user.into();
        let pass = pass.into();

        Ok(Self { user, pass })
    }
}

impl AuthAttempt {
    pub fn user(&self) -> &str {
        &self.auth.user
    }

    pub fn calc_pwhash(&self) -> String {
        pwhash(&self.auth.pass[..])
    }
}

impl SessionId {
    pub fn new() -> Self {
        Self::from(Uuid::new_v4())
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.as_simple().fmt(f)
    }
}

impl FromStr for SessionId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::try_from(s).map(Self).map_err(|_| ())
    }
}

impl From<Uuid> for SessionId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}
