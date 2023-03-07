use std::str::FromStr;

use sha256::digest;
use base64_light::base64_decode;
use tracing::error;
use uuid::Uuid;

use crate::podsync;

pub struct BasicAuth {
    user: String,
    pass: String,
}

pub struct AuthAttempt {
    auth: BasicAuth,
}

pub struct SessionId(Uuid);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.as_simple().fmt(f)
    }
}

impl BasicAuth {
    pub fn with_path_username(self, username: &str) -> podsync::Result<AuthAttempt> {
        (self.user == username)
            .then(|| AuthAttempt { auth: self })
            .ok_or(podsync::Error::Unauthorized)
    }
}

impl AuthAttempt {
    pub fn user(&self) -> &str {
        &self.auth.user
    }

    pub fn calc_pwhash(&self) -> String {
        digest(self.auth.pass)
    }
}

impl FromStr for BasicAuth {
    type Err = &'static str;

    fn from_str(header: &str) -> Result<Self, Self::Err> {
        let (basic, auth_b64) = header
            .split_once(' ')
            .ok_or("no space in auth header")?;

        if basic != "Basic" {
            return Err("only basic auth supported");
        }

        let auth_bytes = base64_decode(auth_b64);
        let auth = std::str::from_utf8(&auth_bytes)
            .map_err(|e| {
                error!("invalid utf-8 for password: {e:?}");
                "none-utf8 in auth header"
            })?;

        let (user, pass) = auth.split_once(':')
            .ok_or("no colon in auth value")?;

        let user = user.into();
        let pass = pass.into();

        Ok(Self { user, pass })
    }
}

impl TryFrom<&str> for SessionId {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, ()> {
        Uuid::try_from(s).map(Self).map_err(|_| ())
    }
}

impl From<Uuid> for SessionId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}
