use crate::utils::{AppState, RumaRequest, RumaResponse};

use axum::{
    extract::{Path, State, TypedHeader},
    headers,
    http::StatusCode,
    response::IntoResponse,
};
use ruma::{
    api::appservice::{
        ping::send_ping::v1::{Request as PingRequest, Response as PingResponse},
    },
    OwnedTransactionId,
};
use tracing::*;

use std::time::{Duration, Instant};

const PING_TIMEOUT: Duration = Duration::from_millis(15000);

#[instrument(skip(registration, authorization, ping_transactions))]
#[axum::debug_handler]
pub async fn handle_ping(
    State(AppState {
        registration,
        ping_transactions,
        ..
    }): State<AppState>,
    TypedHeader(authorization): TypedHeader<headers::Authorization<headers::authorization::Bearer>>,
    RumaRequest(request): RumaRequest<PingRequest>,
) -> Result<RumaResponse<PingResponse>, impl IntoResponse> {
    //) -> Result<RumaResponse<PingResponse>, RumaError> {
    if registration.hs_token != authorization.0.token() {
        warn!("homeserver token in registration and ping don't match");
        return Err("no, brotha.".into_response());
    }

    if let Some(transaction_id) = request.transaction_id {
        if ping_transactions
            .lock()
            .expect("could not get ping transactions")
            .remove(&transaction_id)
            .as_ref()
            .map(Instant::elapsed)
            .map_or(true, |duration| duration > PING_TIMEOUT)
        {
            warn!(?transaction_id, "invalid transaction id");
        }
    }

    Ok(RumaResponse(PingResponse::new()))
}

#[instrument(skip(registration))]
pub async fn handle_transactions(
    State(AppState { registration, .. }): State<AppState>,
    Path(transaction_id): Path<OwnedTransactionId>,
) -> impl IntoResponse {
    StatusCode::OK
}
