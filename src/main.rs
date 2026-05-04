#![cfg_attr(feature = "backend-sql", allow(unexpected_cfgs))]
#![cfg_attr(not(feature = "backend-sql"), deny(unexpected_cfgs))]

use std::{net::SocketAddr, path::Path, sync::Arc};

use ::time::ext::NumericalDuration;
use axum::{
    extract::{ConnectInfo, Path as AxumPath, Query, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{Json, Response},
    routing::{get, post},
    RequestExt as _, Router,
};
use cookie::{Cookie, SameSite};
use log::{debug, error, info};
use serde::Deserialize;

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

mod backend;
use backend::Backend;

static COOKIE_NAME: &str = "sessionid"; // gpodder/mygpo, doc/api/reference/auth.rst:16

#[derive(Debug, Deserialize)]
pub struct QuerySince {
    since: crate::time::Timestamp,
}

#[derive(Clone)]
struct AppState {
    podsync: Arc<PodSync>,
    secure: bool,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let args = <Args as clap::Parser>::parse();

    if args.show_version() {
        println!("PodSync {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let data_dir = args.data_dir().unwrap_or_else(|| Path::new("."));

    let backend = Backend::new(&data_dir).await;

    let secure = args.secure();
    let podsync = Arc::new(PodSync::new(backend));

    let app = routes(podsync, secure);

    let addr = args.addr().expect("couldn't parse address");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("couldn't bind");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("server error");
}

fn routes(podsync: Arc<PodSync>, secure: bool) -> Router {
    let state = AppState { podsync, secure };

    Router::new()
        .route("/", get(hello))
        .route("/api/2/auth/:username/login.json", post(login))
        .route("/api/2/auth/:username/logout.json", post(logout))
        .route("/api/2/devices/:username_format", get(get_devices))
        .route(
            "/api/2/devices/:username/:device_format",
            post(update_device),
        )
        .route(
            "/api/2/subscriptions/:username/:device_format",
            get(get_subscriptions).post(update_subscriptions),
        )
        .route(
            "/api/2/episodes/:username_format",
            get(get_episodes).post(update_episodes),
        )
        .layer(middleware::from_fn(log_middleware))
        .with_state(state)
}

async fn hello() -> &'static str {
    "PodSync is Working!"
}

async fn login(
    State(state): State<AppState>,
    AxumPath(username): AxumPath<String>,
    headers: HeaderMap,
) -> Result<(HeaderMap, StatusCode), podsync::Error> {
    let auth_header = headers.get(header::AUTHORIZATION).ok_or_else(|| {
        error!("couldn't auth {:?} - no auth header/cookie", username);
        podsync::Error::Unauthorized
    })?;

    let auth_str = auth_header
        .to_str()
        .map_err(|_| podsync::Error::Unauthorized)?;
    let auth: BasicAuth = auth_str.parse().map_err(|_| podsync::Error::Unauthorized)?;
    let auth = auth.with_path_username(&username).map_err(|e| {
        error!("{e}");
        podsync::Error::Unauthorized
    })?;

    let session_id = extract_session_id(&headers);
    let authed = state.podsync.login(auth, session_id).await?;
    let session_id = authed.session_id();

    let cookie = Cookie::build((COOKIE_NAME, session_id.to_string()))
        .secure(state.secure)
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(2.weeks())
        .path("/api");

    let cookie_val =
        HeaderValue::from_str(&cookie.to_string()).map_err(|_| podsync::Error::Internal)?;
    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::SET_COOKIE, cookie_val);

    Ok((response_headers, StatusCode::OK))
}

async fn logout(
    State(state): State<AppState>,
    AxumPath(username): AxumPath<String>,
    headers: HeaderMap,
) -> Result<StatusCode, podsync::Error> {
    let authed = authorize_request(&state.podsync, &username, &headers).await?;
    authed.logout().await?;
    Ok(StatusCode::OK)
}

async fn get_devices(
    State(state): State<AppState>,
    AxumPath(username_format): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<Vec<device::DeviceAndSub>>, podsync::Error> {
    let username = split_format_json(&username_format)?;
    let authed = authorize_request(&state.podsync, username, &headers).await?;
    let devs = authed.devices().await?;
    Ok(Json(devs))
}

async fn update_device(
    State(state): State<AppState>,
    AxumPath((username, device_format)): AxumPath<(String, String)>,
    headers: HeaderMap,
    Json(device): Json<device::DeviceUpdate>,
) -> Result<StatusCode, podsync::Error> {
    let device_id = split_format_json(&device_format)?;
    let authed = authorize_request(&state.podsync, &username, &headers).await?;
    authed.update_device(device_id, device).await?;
    Ok(StatusCode::OK)
}

async fn get_subscriptions(
    State(state): State<AppState>,
    AxumPath((username, device_format)): AxumPath<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<QuerySince>,
) -> Result<Json<subscription::SubscriptionChangesToClient>, podsync::Error> {
    let device_id = split_format_json(&device_format)?;
    let authed = authorize_request(&state.podsync, &username, &headers).await?;
    let result = authed.subscriptions(device_id, query.since).await?;
    Ok(Json(result))
}

async fn update_subscriptions(
    State(state): State<AppState>,
    AxumPath((username, device_format)): AxumPath<(String, String)>,
    headers: HeaderMap,
    Json(changes): Json<subscription::SubscriptionChangesFromClient>,
) -> Result<Json<podsync::UpdatedUrls>, podsync::Error> {
    let device_id = split_format_json(&device_format)?;
    let authed = authorize_request(&state.podsync, &username, &headers).await?;
    let result = authed.update_subscriptions(device_id, changes).await?;
    Ok(Json(result))
}

async fn get_episodes(
    State(state): State<AppState>,
    AxumPath(username_format): AxumPath<String>,
    headers: HeaderMap,
    Query(query): Query<podsync::QueryEpisodes>,
) -> Result<Json<episode::Episodes>, podsync::Error> {
    let username = split_format_json(&username_format)?;
    let authed = authorize_request(&state.podsync, username, &headers).await?;
    let result = authed.episodes(query).await?;
    Ok(Json(result))
}

async fn update_episodes(
    State(state): State<AppState>,
    AxumPath(username_format): AxumPath<String>,
    headers: HeaderMap,
    Json(body): Json<Vec<episode::Episode>>,
) -> Result<Json<podsync::UpdatedUrls>, podsync::Error> {
    let username = split_format_json(&username_format)?;
    let authed = authorize_request(&state.podsync, username, &headers).await?;
    let result = authed.update_episodes(body).await?;
    Ok(Json(result))
}

fn extract_session_id(headers: &HeaderMap) -> Option<SessionId> {
    let cookie_header = headers.get(header::COOKIE)?;
    let cookie_str = cookie_header.to_str().ok()?;

    for part in cookie_str.split(';') {
        let part = part.trim();
        if let Some((name, value)) = part.split_once('=') {
            if name.trim() == COOKIE_NAME {
                if let Ok(session_id) = value.trim().parse() {
                    return Some(session_id);
                }
            }
        }
    }
    None
}

async fn authorize_request(
    podsync: &Arc<PodSync>,
    username: &str,
    headers: &HeaderMap,
) -> podsync::Result<PodSyncAuthed<true>> {
    if let Some(session_id) = extract_session_id(headers) {
        // Cookie present: authenticate via session only, no fallback to basic auth
        let authed = podsync.authenticate(session_id).await?;
        return authed
            .with_user(username)
            .map(|a| {
                debug!("authed (via cookie) user {}", a.username());
                a
            })
            .map_err(|e| {
                debug!("no auth via cookie");
                e
            });
    }

    // No cookie: require Authorization header
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .ok_or(podsync::Error::Unauthorized)?;
    let auth_str = auth_header
        .to_str()
        .map_err(|_| podsync::Error::Unauthorized)?;

    info!("login auth");
    let auth: BasicAuth = auth_str.parse().map_err(|_| podsync::Error::Unauthorized)?;
    let auth = auth.with_path_username(username).map_err(|e| {
        error!("{e}");
        podsync::Error::Unauthorized
    })?;

    podsync.login(auth, None).await
}

async fn log_middleware(mut req: Request, next: Next) -> Response {
    use std::fmt::{self, Display, Formatter};

    struct OptFmt<T>(Option<T>);

    impl<T: Display> Display for OptFmt<T> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            match &self.0 {
                Some(x) => x.fmt(f),
                None => write!(f, "-"),
            }
        }
    }

    let method = req.method().clone();
    let uri = req.uri().clone();
    let version = req.version();
    let referer = req
        .headers()
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let user_agent = req
        .headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let remote_addr: Option<SocketAddr> = req
        .extract_parts()
        .await
        .map_err(|e| {
            error!("couldn't extract address from request: {e}");
        })
        .ok()
        .map(|ci: ConnectInfo<_>| ci.0);

    let start = std::time::Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed();

    let now = Timestamp::now();

    info!(
        target: "podsync::http",
        "{} {} \"{} {} {:?}\" {} \"{}\" \"{}\" {:?}",
        OptFmt(remote_addr),
        match now {
            Ok(t) => t.to_string(),
            Err(e) => {
                error!("couldn't get time: {e:?}");
                "<notime>".into()
            }
        },
        method,
        uri,
        version,
        response.status().as_u16(),
        OptFmt(referer),
        OptFmt(user_agent),
        elapsed,
    );

    response
}

#[cfg(test)]
#[cfg(feature = "backend-sql")]
mod test {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use sqlx::query;
    use tower::ServiceExt;

    use super::*;
    use base64_light::base64_encode as base64;

    #[tokio::test]
    async fn hello() {
        let db = backend::test::create_db().await;
        let podsync = Arc::new(PodSync::new(backend::Backend(db)));
        let app = routes(podsync, true);

        let res = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn login_session() {
        let db = backend::test::create_db().await;

        // setup bob:abc
        let pass = "abc";
        let pwhash = auth::pwhash(pass);
        query!(
            r#"
            INSERT INTO users
            VALUES ("bob", ?, NULL);
            "#,
            pwhash,
        )
        .execute(&db)
        .await
        .unwrap();

        let app = routes(Arc::new(PodSync::new(backend::Backend(db))), true);
        let bob_auth = format!("Basic {}", base64(&format!("{}:{}", "bob", pass)));

        // logging in succeeds
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/2/auth/bob/login.json")
                    .method("POST")
                    .header("authorization", &bob_auth)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);

        // and we're given a cookie
        let cookie_header = res
            .headers()
            .get("set-cookie")
            .expect("session cookie")
            .clone();
        let cookie = Cookie::parse(cookie_header.to_str().unwrap()).unwrap();

        assert_eq!(cookie.name(), COOKIE_NAME);

        // we can use this to get our devices:
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/2/devices/bob.json")
                    .header("cookie", cookie.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // a POST to /login with the same auth and a cookie will verify the cookie:
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/2/auth/bob/login.json")
                    .method("POST")
                    .header("authorization", &bob_auth)
                    .header("cookie", cookie.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // and a POST to /login with the wrong auth will reject:
        let tim_auth = format!("Basic {}", base64(&format!("{}:{}", "tim", "123")));
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/2/auth/bob/login.json")
                    .method("POST")
                    .header("authorization", &tim_auth)
                    .header("cookie", cookie.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // and logging out will invalidate the session
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/2/auth/bob/logout.json")
                    .method("POST")
                    .header("cookie", cookie.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/2/devices/bob.json")
                    .header("cookie", cookie.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }
}
