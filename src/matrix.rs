use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use ruma::{api::appservice::Registration, TransactionId};
use tracing::*;

#[instrument(skip(registration))]
pub async fn handle_transactions(
    State(registration): State<Registration>,
    Path(transaction_id): Path<Box<TransactionId>>,
) -> impl IntoResponse {
    StatusCode::OK
}
