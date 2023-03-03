use std::str::FromStr;

use base64_light::base64_decode;
use tracing::error;

use crate::podsync;

pub struct Auth {
    user: String,
    pass: String,
}

pub struct Credentials {
    user: String,
    pass: String,
}

impl FromStr for Auth {
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

impl Auth {
    pub fn with_user(self, new_user: String) -> Result<Credentials, podsync::Error> {
        let Self { user, pass } = self;

        if user == new_user {
            Ok(Credentials { user, pass })
        } else {
            Err(podsync::Error::Unauthorized)
        }
    }
}

impl Credentials {
    pub fn user(&self) -> &str { &self.user }
    pub fn pass(&self) -> &str { &self.pass }
}
