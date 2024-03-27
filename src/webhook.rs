use crate::utils::ApiSecret;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use freestuffapi::api::GameId;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tracing::*;

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct Event {
    #[serde(rename = "event")]
    name: String,
    secret: Option<String>,
    data: JsonValue,
}

pub enum EventError {
    Json(serde_json::Error),
    InvalidEvent(String),
    /// Secret configuration was invalid
    BadSecret,
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
            EventError::BadSecret => (StatusCode::UNAUTHORIZED, "unauthorized").into_response(),
        }
    }
}

#[instrument(skip_all)]
pub async fn handle_webhooks(
    State(secret): State<Option<ApiSecret>>,
    Json(event): Json<Event>,
) -> Result<impl IntoResponse, EventError> {
    let secret = secret.map(|s| s.0);
    match (&secret, event.secret) {
        (Some(configured), Some(event)) if *configured != event => {
            warn!("incorrect secret");
            return Err(EventError::BadSecret);
        }
        (Some(_configured), None) => {
            warn!("no secret set for event");
            return Err(EventError::BadSecret);
        }
        (None, Some(_event)) => warn!("event had secret, but none is configured"),
        (Some(_), Some(_)) | (None, None) => {
            trace!(required = secret.is_some(), "valid secret");
        }
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
