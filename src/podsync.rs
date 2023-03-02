use sqlx::{Pool, Database, Executor, query_as};
use warp::http::StatusCode;

use crate::device::Device;

pub struct PodSync<DB: Database>(Pool<DB>);

impl<'c, DB: Database + Executor<'c>> PodSync<DB> {
    pub fn new(db: Pool<DB>) -> Self {
        Self(db)
    }

    pub async fn login(&self, username: String, auth: String)
        -> Result<(), StatusCode>
    {
        eprintln!("todo: auth or {username}: {auth}");

        Ok(())
    }

    pub async fn devices(&self, username_format: String) -> Result<(), StatusCode> {
        // let (username, format) = split_format(username_format)?; // FIXME: ? -> return 40?
        let (username, format) = split_dot(&username_format)?;
        err_unless_json(format)?;

        let query = query_as!(
                Device,
                r#"
                SELECT id, caption, type as "type: _", subscriptions, username
                FROM devices
                WHERE username = ?
                "#,
                username,
        )
                .fetch_all(&mut self.0)
                .await;

        let devices = match query {
            Ok(d) => d,
            Err(e) => {
                // error!("select error: {:?}", e);

                // return warp::reply::with_status(
                //     warp::reply(),
                //     warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                //     ).into_response();
                todo!()
            }
        };

        todo!()
        // warp::reply::json(&devices).into_response()
    }
}

fn split_dot(s: &str) -> Result<(&str, &str), StatusCode> {
    s.split_once('.').ok_or(StatusCode::BAD_REQUEST)
}

fn err_unless_json(s: &str) -> Result<(), StatusCode> {
    (s == "json")
        .then_some(())
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)
}
