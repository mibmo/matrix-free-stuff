use crate::utils::RumaRequest;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use ruma::{
    api::appservice::{
        ping::send_ping::v1::{Request as PingRequest, Response as PingResponse},
        Registration,
    },
    OwnedTransactionId,
};
use tracing::*;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct PingTransactions(pub Vec<OwnedTransactionId>);

#[instrument]
#[axum::debug_handler]
pub async fn handle_ping(
    //State(ping_transactions): State<PingTransactions>,
    RumaRequest(request): RumaRequest<PingRequest>
) -> impl IntoResponse {
    StatusCode::OK
}

#[instrument(skip(registration))]
pub async fn handle_transactions(
    State(registration): State<Registration>,
    Path(transaction_id): Path<OwnedTransactionId>,
) -> impl IntoResponse {
    StatusCode::OK
}
