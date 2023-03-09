use std::{
    sync::Arc,
    future::Future,
};

use time::{OffsetDateTime, ext::NumericalDuration};
use warp::{Filter, Reply, http::{self, header::{HeaderMap, HeaderValue}}, hyper::Body};
use cookie::{Cookie, SameSite};
use serde::{Deserialize, Serialize};

use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};

use log::info;

mod auth;
use auth::{BasicAuth, SessionId};

mod user;

mod device;

mod subscription;
use subscription::SubscriptionChanges;

mod episode;
use episode::EpisodeChangeWithDevice;

mod podsync;
use podsync::{PodSync, PodSyncAuthed};

mod path_format;
use path_format::split_format_json;

mod args;
use args::Args;

static DB_URL: &str = "sqlite://pod.sql";
static COOKIE_NAME: &str = "sessionid"; // gpodder/mygpo, doc/api/reference/auth.rst:16

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let args = <Args as clap::Parser>::parse();

    match Sqlite::create_database(DB_URL).await {
        Ok(()) => {
            info!("Using {}", DB_URL);
        }
        Err(e) => {
            let sqlx::Error::Database(db_err) = e else {
                panic!("error creating database: {e}");
            };

            panic!("sql db error: {db_err:?}");//.code()
        }
    }

    let db = SqlitePool::connect(DB_URL)
        .await
        .expect("DB connection");

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("migration");

    let secure = args.secure();
    let podsync = Arc::new(PodSync::new(db));

    let auth_check = warp::cookie(COOKIE_NAME)
        .and_then({
            let podsync = Arc::clone(&podsync);

            move |session_id: SessionId| {
                let podsync = Arc::clone(&podsync);

                async move {
                    podsync
                        .authenticate(session_id)
                        .await
                        .map_err(warp::reject::custom)
                }
            }
        });

    let auth = {
        let login =
            warp::post()
            .and(warp::path!("api" / "2" / "auth" / String / "login.json"))
            .and(warp::header::optional("authorization"))
            .and(warp::cookie::optional(COOKIE_NAME))
            .then({
                let podsync = Arc::clone(&podsync);
                move |username: String, auth: Option<BasicAuth>, session_id: Option<SessionId>| {
                    let podsync = Arc::clone(&podsync);

                    result_to_headers(async move {
                        let auth = match auth {
                            Some(auth) => auth.with_path_username(&username),
                            None => Err(podsync::Error::Unauthorized),
                        }?;

                        let podsync = podsync
                            .login(auth, session_id)
                            .await?;
                        let session_id = podsync.session_id();

                        let cookie = Cookie::build(COOKIE_NAME, session_id.to_string())
                            .secure(secure)
                            .http_only(true)
                            .same_site(SameSite::Strict)
                            .max_age(2.weeks())
                            .path("/api")
                            .finish();

                        let cookie = HeaderValue::from_str(&cookie.to_string())
                            .map_err(|_| podsync::Error::Internal)?;
                        let mut headers = HeaderMap::new();
                        headers.insert("set-cookie", cookie);
                        Ok(headers)
                    })
                }
            });

        let logout =
            warp::post()
            .and(warp::path!("api" / "2" / "auth" / String / "logout.json"))
            .and(auth_check.clone())
            .then(move |username: String, podsync: PodSyncAuthed| result_to_ok(async move {
                podsync
                    .with_user(&username)?
                    .logout()
                    .await
            }));

        login.or(logout)
    };

    let devices = {
        let for_user = warp::path!("api" / "2" / "devices" / String)
            .and(warp::get())
            .and(auth_check.clone())
            .then(|username_format: String, podsync: PodSyncAuthed| result_to_json(async move {
                let username = split_format_json(&username_format)?;

                let devs = podsync
                    .with_user(username)?
                    .devices()
                    .await?;

                Ok(devs)
            }));

        let create = warp::path!("api" / "2" / "devices" / String / String)
            .and(warp::post())
            .and(auth_check.clone())
            .and(warp::body::json()) // TODO: this may just be an empty string
            .then(move |username: String, device_name, podsync: PodSyncAuthed, device| {
                result_to_json(async move {
                    podsync.with_user(&username)?
                        .update_device(device_name, device)
                        .await
                })
            });

        for_user.or(create)
    };

    let subscriptions = {
        let get = warp::path!("api" / "2" / "subscriptions" / String / String)
            // FIXME: merge this ^
            // with the below path (same for /episodes)
            .and(warp::get())
            .and(auth_check.clone())
            .then(move |username: String, deviceid_format: String, podsync: PodSyncAuthed| {
                result_to_json(async move {
                    let device_id = split_format_json(&deviceid_format)?;
                    podsync.with_user(&username)?
                        .subscriptions(device_id)
                        .await
                })
            });

        let upload = warp::path!("api" / "2" / "subscriptions" / String / String)
            .and(warp::post())
            .and(auth_check.clone())
            .and(warp::body::json())
            .then(move |username: String, deviceid_format: String, podsync: PodSyncAuthed, changes: SubscriptionChanges| {
                result_to_json(async move {
                    let device_id = split_format_json(&deviceid_format)?;

                    podsync.with_user(&username)?
                        .update_subscriptions(device_id, changes)
                        .await
                })
            });

        get.or(upload)
    };

    let episodes = {
        let get = warp::path!("api" / "2" / "episodes" / String)
            .and(warp::get())
            .and(warp::query())
            .and(auth_check.clone())
            .then(move |username_format: String, query: QuerySince, podsync: PodSyncAuthed| {
                result_to_json(async move {
                    let username = split_format_json(&username_format)?;

                    podsync.with_user(&username)?
                        .episodes(query)
                        .await
                })
            });

        let upload = warp::path!("api" / "2" / "episodes" / String)
            .and(warp::post())
            .and(auth_check.clone())
            .and(warp::body::json())
            .then(move |username_format: String, podsync: PodSyncAuthed, body: Vec<EpisodeChangeWithDevice>| {
                result_to_json(async move {
                    let username = split_format_json(&username_format)?;

                    podsync.with_user(&username)?
                        .update_episodes(body)
                        .await
                })
            });

        get.or(upload)
    };

    let routes = auth
        .or(devices)
        .or(subscriptions)
        .or(episodes)
        .with(warp::log::custom(|info| {
            use std::fmt::*;
            use time::format_description::well_known::Rfc3339;

            struct OptFmt<T>(Option<T>);

            impl<T: Display> Display for OptFmt<T> {
                fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
                    match self.0 {
                        Some(ref x) => x.fmt(fmt),
                        None => write!(fmt, "-"),
                    }
                }
            }

            let now = match OffsetDateTime::now_local() {
                Ok(now) => now.format(&Rfc3339).ok(),
                Err(_) => None,
            };

            info!(
                target: "podsync::warp",
                "{} {} \"{} {} {:?}\" {} \"{}\" \"{}\" {:?}",
                OptFmt(info.remote_addr()),
                OptFmt(now),
                info.method(),
                info.path(),
                info.version(),
                info.status().as_u16(),
                OptFmt(info.referer()),
                OptFmt(info.user_agent()),
                info.elapsed(),
            );
        }));

    warp::serve(routes)
        .run(args.addr())
        .await;
}

async fn result_to_json<F, B>(f: F) -> impl warp::Reply
where
    F: Future<Output = podsync::Result<B>>,
    B: Serialize,
{
    match f.await {
        Ok(body) => warp::reply::json(&body).into_response(),
        Err(e) => err_to_warp(e).into_response(),
    }
}

async fn result_to_ok<F>(f: F) -> impl warp::Reply
where
    F: Future<Output = podsync::Result<()>>,
{
    match f.await {
        Ok(()) => warp::reply().into_response(),
        Err(e) => err_to_warp(e).into_response(),
    }
}

async fn result_to_headers<F>(f: F) -> impl warp::Reply
where
    F: Future<Output = podsync::Result<HeaderMap>>,
{
    match f.await {
        Ok(header_map) => {
            let mut resp = http::Response::builder();

            match resp.headers_mut() {
                Some(headers) => headers.extend(header_map),
                None => {
                    for (name, value) in header_map {
                        if let Some(name) = name {
                            resp = resp.header(name, value);
                        }
                    }
                }
            }

            resp.body(Body::empty()).unwrap()
        }
        Err(e) => err_to_warp(e).into_response(),
    }
}

fn err_to_warp(e: podsync::Error) -> impl warp::Reply {
    warp::reply::with_status(warp::reply(), e.into())
}

#[derive(Debug, Deserialize, Serialize)]
pub struct QuerySince {
    since: u32,
}
