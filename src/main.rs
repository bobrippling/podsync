use std::{
    sync::Arc,
    future::Future,
};

use warp::{Filter, Reply, http::header::{HeaderMap, HeaderValue}};
use serde::{Deserialize, Serialize};

use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};

use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

mod auth;
use auth::{BasicAuth, SessionId};

mod user;
use user::User;

mod device;

mod subscription;
use subscription::SubscriptionChanges;

mod episode;
use episode::EpisodeChangeWithDevice;

mod podsync;
use podsync::PodSync;

mod path_format;
use path_format::split_format_json;

static DB_URL: &str = "sqlite://pod.sql";
static COOKIE_NAME: &str = "sessionid"; // gpodder/mygpo, doc/api/reference/auth.rst:16

#[tokio::main]
async fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    match Sqlite::create_database(DB_URL).await {
        Ok(()) => {
            info!("Created database {}", DB_URL);
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

    let podsync = Arc::new(PodSync::new(db));

    let auth = {
        let login =
            warp::post()
            .and(warp::path!("api" / "2" / "auth" / String / "login.json"))
            .and(warp::header::optional("authorization"))
            .and(warp::cookie::optional(COOKIE_NAME))
            .then({
                let podsync = Arc::clone(&podsync);
                move |username: String, auth: Option<BasicAuth>, session_id: Option<String>| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let auth = match auth {
                            Some(auth) => auth.with_path_username(&username),
                            None => Err(podsync::Error::Unauthorized),
                        }?;

                        let maybe_session_id = session_id
                            .as_deref()
                            .map(SessionId::try_from)
                            .transpose()
                            .map_err(|()| podsync::Error::BadRequest)?;

                        let podsync = podsync
                            .login(&auth, maybe_session_id)
                            .await?;
                        let session_id = podsync.session_id();

                        let headers = HeaderMap::new();
                        headers.insert(
                            COOKIE_NAME,
                            HeaderValue::from_str(&session_id.to_string())
                                .map_err(|_| podsync::Error::Internal)?,
                        );

                        Ok((
                            (),
                            headers,
                        ))
                    })
                }
            });

        // TODO: logout (https://github.com/gpodder/mygpo/blob/HEAD/doc/api/reference/auth.rst)
        //login.or(logout)
        login
    };

    let auth_check = warp::cookie::<String>(COOKIE_NAME)
        .then(|cookie| {
            async move {
                println!("got cookie: {cookie}");

                // TODO: lookup user with session_id == {cookie} in DB and return some struct representing them
                // User {
                //     username: "uname_placeholder".to_string(),
                //     pwhash: "ad08awd".to_string(),
                // }
                todo!()
            }
        });

    let devices = {
        let for_user = warp::path!("api" / "2" / "devices" / String)
            .and(warp::get())
            .and(auth_check)
            .then({
                let podsync = Arc::clone(&podsync);
                move |username_format: String, user: User| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let username = split_format_json(&username_format)?;

                        if user.username != username {
                            return Err(podsync::Error::Unauthorized);
                        }

                        // let credentials = Credentials { // user.with_user(username.to_string())?;
                        //     user: "un".to_string(),
                        //     pass: "pw".to_string(),
                        // };

                        podsync.authenticate(todo!())
                            .await?
                            .devices(username_format)
                            .await
                    })
                }
            });

        /*
        let create = warp::path!("api" / "2" / "devices" / String / String)
            .and(warp::post())
            .and(auth_check)
            .and(warp::body::json()) // TODO: this may just be an empty string
            .then({
                let podsync = Arc::clone(&podsync);
                move |username, device_name, auth: Option<BasicAuth>, device| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let credentials = auth.with_user(username)?;

                        podsync.authenticate(&credentials)
                            .await?
                            .create_device(device_name, device)
                            .await
                    })
                }
            });

        for_user.or(create)
        */
        for_user
    };

    /*
    let subscriptions = {
        let get = warp::path!("api" / "2" / "subscriptions" / String / String)
            // FIXME: merge this ^
            // with the below path (same for /episodes)
            .and(warp::get())
            .and(auth_check)
            .then({
                let podsync = Arc::clone(&podsync);
                move |username, deviceid_format: String, auth: Option<BasicAuth>| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let device_id = split_format_json(&deviceid_format)?;
                        let credentials = auth.with_user(username)?;

                        podsync.authenticate(&credentials)
                            .await?
                            .subscriptions(device_id)
                            .await
                    })
                }
            });

        let upload = warp::path!("api" / "2" / "subscriptions" / String / String)
            .and(warp::post())
            .and(auth_check)
            .and(warp::body::json())
            .then({
                let podsync = Arc::clone(&podsync);
                move |username, deviceid_format: String, auth: Option<BasicAuth>, changes: SubscriptionChanges| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let credentials = auth.with_user(username)?;

                        podsync.authenticate(&credentials)
                            .await?
                            .update_subscriptions(deviceid_format, changes)
                            .await
                    })
                }
            });

        get.or(upload)
    };

    let episodes = {
        let get = warp::path!("api" / "2" / "episodes" / String)
            .and(warp::get())
            .and(warp::query())
            .and(auth_check)
            .then({
                let podsync = Arc::clone(&podsync);
                move |username_format: String, query: QuerySince, user: User| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let username = split_format_json(&username_format)?;
                        let credentials = user.with_user(username.to_string())?;

                        podsync.authenticate(&credentials)
                            .await?
                            .episodes(query)
                            .await
                    })
                }
            });

        let upload = warp::path!("api" / "2" / "episodes" / String)
            .and(warp::post())
            .and(auth_check)
            .and(warp::body::json())
            .then({
                let podsync = Arc::clone(&podsync);
                move |username_format: String, auth: Option<BasicAuth>, body: Vec<EpisodeChangeWithDevice>| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let username = split_format_json(&username_format)?;
                        let credentials = auth.with_user(username.to_string())?;

                        podsync.authenticate(&credentials)
                            .await?
                            .update_episodes(body)
                            .await
                    })
                }
            });

        get.or(upload)
    };
    */

    let log = warp::log::custom(|info| {
        println!("request headers:");
        for (key, value) in info.request_headers() {
            println!("  {:?}: {:?}", key, value);
        }
    });

    let routes = auth
        .or(devices)
        // .or(subscriptions)
        // .or(episodes)
        .with(warp::trace::request())
        .with(log);

    warp::serve(routes)
        .run(([0, 0, 0, 0], 8080))
        .await;
}

async fn result_to_response<F, S>(f: F) -> impl warp::Reply
where
    F: Future<Output = podsync::Result<(S, HeaderMap)>>,
    S: Serialize,
{
    match f.await {
        Ok((s, headers)) => {
            let max_age = 1209600;

            // FIXME: iterate over headers

            warp::reply::with_header(
                warp::reply::json(&s),
                "set-cookie",
                format!(
                    "{}={}; Path=/; HttpOnly; Max-Age={}",
                    COOKIE_NAME,
                    "MYCOOKIE",
                    max_age,
                ),
            ).into_response()
        }
        Err(e) => {
            if false && matches!(e, podsync::Error::Unauthorized) {
                warp::reply::with_header(
                    warp::reply::with_status(
                        warp::reply(),
                        e.into()
                    ),
                    "www-authenticate",
                    "Basic realm=\"\""
                )
                    .into_response()
            } else {
                warp::reply::with_status(warp::reply(), e.into())
                    .into_response()
            }
        }
    }
}

async fn result_to_headers<F, S>(f: F) -> impl warp::Reply
where
    F: Future<Output = podsync::Result<S>>,
    S: Serialize,
{
    match f.await {
        Ok(s) => {
            warp::reply::json(&s).into_response()
        }
        Err(e) => {
            if false && matches!(e, podsync::Error::Unauthorized) {
                warp::reply::with_header(
                    warp::reply::with_status(
                        warp::reply(),
                        e.into()
                    ),
                    "www-authenticate",
                    "Basic realm=\"\""
                )
                    .into_response()
            } else {
                warp::reply::with_status(warp::reply(), e.into())
                    .into_response()
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct QuerySince {
    since: u32,
}
