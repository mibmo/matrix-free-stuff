use crate::utils::{AppState, ClientError, RumaError, RumaRequest, RumaResponse};

use axum::{
    extract::{Path, State, TypedHeader},
    headers,
    http::StatusCode,
    response::IntoResponse,
};
use ruma::{
    api::appservice::{event::push_events, ping::send_ping},
    OwnedTransactionId, RoomId,
};
use tracing::*;

#[instrument(skip(registration, authorization))]
#[axum::debug_handler]
pub async fn handle_ping(
    State(AppState { registration, .. }): State<AppState>,
    TypedHeader(authorization): TypedHeader<headers::Authorization<headers::authorization::Bearer>>,
    RumaRequest(request): RumaRequest<send_ping::v1::Request>,
) -> Result<RumaResponse<send_ping::v1::Response>, RumaResponse<ClientError>> {
    if registration.hs_token != authorization.0.token() {
        warn!("homeserver token in registration and ping don't match");
        return Err(RumaResponse(RumaError::Unauthorized.into()));
    }

    Ok(RumaResponse(send_ping::v1::Response::new()))
}

#[instrument(skip(client, request))]
pub async fn handle_transactions(
    State(AppState { client, .. }): State<AppState>,
    Path(transaction_id): Path<OwnedTransactionId>,
    RumaRequest(request): RumaRequest<push_events::v1::Request>,
) -> impl IntoResponse {
    let mut events = request
        .events
        .into_iter()
        .filter_map(|event| event.deserialize().ok());
    while let Some(event) = events.next() {
        use ruma::{api, events::{
            AnyStateEvent::*, AnyTimelineEvent::*, OriginalStateEvent as OSE, StateEvent::*,
            room::member::{RoomMemberEventContent, MembershipState},
        }};

        match event {
            State(RoomMember(Original(OSE {
                room_id,
                sender,
                content: RoomMemberEventContent {
                    membership: MembershipState::Invite,
                    is_direct,
                    ..
                },
                ..
            }))) => {
                trace!(?room_id, ?is_direct, "invited to room");
                let id = RoomId::parse(room_id).unwrap();
                let request = api::client::membership::join_room_by_id::v3::Request::new(id);
                client.send_customized_request(request, |request| {
                    // @TODO: add `via` parameter to query string with same server as inviter
                    Ok(())
                }).await.unwrap();
            },
            _ => debug!("unhandled event"),
        }
    }
    StatusCode::OK
}
