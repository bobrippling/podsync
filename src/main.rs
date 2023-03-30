use std::{future::Future, sync::Arc};

use ::time::ext::NumericalDuration;
use cookie::{Cookie, SameSite};
use serde::{Deserialize, Serialize};
use warp::{
    http::{
        self,
        header::{HeaderMap, HeaderValue},
    },
    hyper::Body,
    Filter, Reply,
};

use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};

use log::{error, info};

mod auth;
use auth::{BasicAuth, SessionId};

mod user;

mod device;

mod subscription;

mod episode;

mod podsync;
use podsync::{PodSync, PodSyncAuthed};

mod time;
use crate::time::Timestamp;

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

            panic!("sql db error: {db_err:?}"); //.code()
        }
    }

    let db = SqlitePool::connect(DB_URL).await.expect("DB connection");

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("migration");

    let secure = args.secure();
    let podsync = Arc::new(PodSync::new(db));

    let hello = warp::path::end()
        .and(warp::get())
        .map(|| "PodSync is Working!");

    let auth = {
        let login = warp::post()
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

                        let podsync = podsync.login(auth, session_id).await?;
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

        let logout = warp::post()
            .and(warp::path!(
                "api" / "2" / "auth" / .. /* String / "logout.json" */
            ))
            .and(authorize(UsernameFormat::Name, podsync.clone()))
            .and(warp::path::path("logout.json").and(warp::path::end()))
            .then(move |podsync: PodSyncAuthed<true>| {
                result_to_ok(async move { podsync.logout().await })
            });

        login.or(logout)
    };

    let devices = {
        let for_user = warp::path!("api" / "2" / "devices" / .. /* String */)
            .and(warp::get())
            .and(authorize(UsernameFormat::NameJson, podsync.clone()))
            .and(warp::path::end())
            .then(|podsync: PodSyncAuthed<true>| {
                result_to_json(async move {
                    let devs = podsync.devices().await?;

                    Ok(devs)
                })
            });

        let update = warp::path!("api" / "2" / "devices" / .. /* String / String */)
            .and(warp::post())
            .and(authorize(UsernameFormat::Name, podsync.clone()))
            .and(warp::path::param::<String>().and(warp::path::end()))
            .and(warp::body::json())
            .then(
                move |podsync: PodSyncAuthed<true>, deviceid_format: String, device| {
                    result_to_ok(async move {
                        let device_id = split_format_json(&deviceid_format)?;
                        podsync.update_device(device_id, device).await
                    })
                },
            );

        for_user.or(update)
    };

    let subscriptions = {
        let get = warp::path!("api" / "2" / "subscriptions" / .. /* String / String*/)
            .and(warp::get())
            .and(authorize(UsernameFormat::Name, podsync.clone()))
            .and(warp::path::param::<String>().and(warp::path::end()))
            .and(warp::query())
            .then(
                move |podsync: PodSyncAuthed<true>, deviceid_format: String, query: QuerySince| {
                    result_to_json(async move {
                        let device_id = split_format_json(&deviceid_format)?;
                        podsync.subscriptions(device_id, query.since).await
                    })
                },
            );

        let upload = warp::path!("api" / "2" / "subscriptions" / .. /* String / String */)
            .and(warp::post())
            .and(authorize(UsernameFormat::Name, podsync.clone()))
            .and(warp::path::param::<String>().and(warp::path::end()))
            .and(warp::body::json())
            .then(
                move |podsync: PodSyncAuthed<true>, deviceid_format: String, changes| {
                    result_to_json(async move {
                        let device_id = split_format_json(&deviceid_format)?;

                        podsync.update_subscriptions(device_id, changes).await
                    })
                },
            );

        get.or(upload)
    };

    let episodes = {
        let get = warp::path!("api" / "2" / "episodes" / .. /* String */)
            .and(warp::get())
            .and(authorize(UsernameFormat::NameJson, podsync.clone()))
            .and(warp::path::end())
            .and(warp::query())
            .then(
                move |podsync: PodSyncAuthed<true>, query: podsync::QueryEpisodes| {
                    result_to_json(async move { podsync.episodes(query).await })
                },
            );

        let upload = warp::path!("api" / "2" / "episodes" / .. /* String */)
            .and(warp::post())
            .and(authorize(UsernameFormat::NameJson, podsync.clone()))
            .and(warp::path::end())
            .and(warp::body::json())
            .then(move |podsync: PodSyncAuthed<true>, body| {
                result_to_json(async move { podsync.update_episodes(body).await })
            });

        get.or(upload)
    };

    let routes = hello
        .or(auth)
        .or(devices)
        .or(subscriptions)
        .or(episodes)
        .with(warp::log::custom(|info| {
            use std::fmt::*;

            struct OptFmt<T>(Option<T>);

            impl<T: Display> Display for OptFmt<T> {
                fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
                    match self.0 {
                        Some(ref x) => x.fmt(fmt),
                        None => write!(fmt, "-"),
                    }
                }
            }

            let now = Timestamp::now();

            info!(
                target: "podsync::warp",
                "{} {} \"{} {} {:?}\" {} \"{}\" \"{}\" {:?}",
                OptFmt(info.remote_addr()),
                match now {
                    Ok(t) => t.to_string(),
                    Err(e) => {
                        error!("couldn't get time: {e:?}");
                        "<notime>".into()
                    }
                },
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
        .run(args.addr().expect("couldn't parse address"))
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

#[derive(Debug, Deserialize)]
pub struct QuerySince {
    since: crate::time::Timestamp,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum UsernameFormat {
    Name,
    NameJson,
}
impl UsernameFormat {
    pub fn convert<'a>(&self, username: &'a str) -> Result<&'a str, warp::Rejection> {
        match self {
            Self::Name => Ok(username),
            Self::NameJson => split_format_json(username).map_err(warp::reject::custom),
        }
    }
}

fn cookie_authorize(
    username_fmt: UsernameFormat,
    podsync: Arc<PodSync>,
) -> impl Filter<Extract = (PodSyncAuthed<true>,), Error = warp::Rejection> + Clone {
    let cookie_auth_check = warp::cookie(COOKIE_NAME).and_then({
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

    warp::path::param::<String>()
        .and(cookie_auth_check)
        .and_then(move |username: String, podsync: PodSyncAuthed| async move {
            podsync
                .with_user(username_fmt.convert(&username)?)
                .map_err(warp::reject::custom)
        })
}

fn login_authorize(
    username_fmt: UsernameFormat,
    podsync: Arc<PodSync>,
) -> impl Filter<Extract = (PodSyncAuthed<true>,), Error = warp::Rejection> + Clone {
    warp::path::param::<String>()
        .and(warp::header("authorization"))
        .and(warp::cookie::optional(COOKIE_NAME))
        .and_then(
            move |username: String, auth: BasicAuth, session_id: Option<SessionId>| {
                let podsync = Arc::clone(&podsync);
                async move {
                    let username = username_fmt.convert(&username)?;
                    let auth = auth.with_path_username(&username)?;
                    podsync
                        .login(auth, session_id)
                        .await?
                        .with_user(&username)
                        .map_err(warp::reject::custom)
                }
            },
        )
}

fn authorize(
    username_fmt: UsernameFormat,
    podsync: Arc<PodSync>,
) -> impl Filter<Extract = (PodSyncAuthed<true>,), Error = warp::Rejection> + Clone {
    cookie_authorize(username_fmt, podsync.clone())
        .or(login_authorize(username_fmt, podsync.clone()))
        .unify()
}
