use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use freestuffapi::api::GameId;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value as JsonValue;
use eyre::Result as EResult;
use tracing::*;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
struct ApiSecret(pub String);

#[tokio::main]
#[instrument]
async fn main() -> EResult<()> {
    tracing_subscriber::fmt::init();

    let webhook_path = std::env::var("WEBHOOK_PATH").unwrap_or("/".to_string());
    let webhook_secret = std::env::var("WEBHOOK_SECRET")
        .map(ApiSecret)
        .map_err(|_| warn!("no secret specified"))
        .ok();

    let app = Router::new()
        .route(&webhook_path, get(handle_webhooks))
        .route(&webhook_path, post(handle_webhooks))
        .with_state(webhook_secret);

    let addr = std::env::var("WEBHOOK_ADDR").unwrap_or("0.0.0.0:3000".to_string());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
struct Event {
    #[serde(rename = "event")]
    name: String,
    secret: Option<String>,
    data: JsonValue,
}


enum EventError {
    Json(serde_json::Error),
    InvalidEvent(String),
    /// Secret configuration was invalid
    BadSecret(bool),
}

impl IntoResponse for EventError {
    fn into_response(self) -> Response {
        match self {
            EventError::Json(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "event serialization failed",
            )
                .into_response(),
            EventError::InvalidEvent(name) => {
                (StatusCode::BAD_REQUEST, format!("invalid event: {name}")).into_response()
            }
            EventError::BadSecret(true) => (StatusCode::FORBIDDEN, "bad secret").into_response(),
            EventError::BadSecret(false) => {
                (StatusCode::UNAUTHORIZED, "no secret given").into_response()
            }
        }
    }
}

#[instrument(skip_all)]
async fn handle_webhooks(
    State(secret): State<Option<ApiSecret>>,
    Json(event): Json<Event>,
) -> Result<impl IntoResponse, EventError> {
    let secret = secret.map(|s| s.0);
    match (secret, event.secret) {
        (Some(configured), Some(event)) if configured != event => {
            warn!("incorrect secret");
            return Err(EventError::BadSecret(true));
        }
        (Some(_configured), None) => {
            warn!("no secret set for event");
            return Err(EventError::BadSecret(false));
        }
        (None, Some(_event)) => warn!("event had secret, but none is configured"),
        _ => {} // cases left: both set to same & both none
    }

    match event.name.as_str() {
        "free_games" => {
            let games = handler_data_from_json_value(event.data)?;
            Ok(hook_free_games(games).await.into_response())
        }
        name => {
            error!(event = name, "invalid event");
            Err(EventError::InvalidEvent(name.to_string()))
        }
    }
}

#[instrument(skip_all)]
fn handler_data_from_json_value<T: DeserializeOwned>(value: JsonValue) -> Result<T, EventError> {
    serde_json::from_value(value).map_err(|error| {
        error!(?error, "failed to deserialize handler data");
        EventError::Json(error)
    })
}

#[instrument]
async fn hook_free_games(games: Vec<GameId>) -> StatusCode {
    StatusCode::OK
}
