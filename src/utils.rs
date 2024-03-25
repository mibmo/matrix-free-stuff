use axum::{
    async_trait,
    extract::{FromRequest, FromRequestParts, OriginalUri, Path},
    http::{self, request::Parts, Request, StatusCode, Uri},
    response::{IntoResponse, Response},
    BoxError,
};
use ruma::{
    api::{
        appservice::Registration,
        client::{
            error::{ErrorBody, ErrorKind},
            Error as ClientError,
        },
        IncomingRequest, OutgoingResponse,
    },
    OwnedTransactionId,
};
use serde_json::json;
use tracing::*;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct AppState {
    pub registration: Registration,
    pub ping_transactions: Arc<Mutex<HashMap<OwnedTransactionId, Instant>>>,
}

pub struct RumaRequest<T: IncomingRequest>(pub T);

impl<T: IncomingRequest> RumaRequest<T> {
    pub async fn new<S: Send + Sync>(
        body: Vec<u8>,
        uri: &Uri,
        mut req: Parts,
        state: &S,
    ) -> Result<Self, ruma::api::client::Error> {
        // make a mock request to use with T::try_from_http_request
        let mut new_request = http::Request::builder().method(req.method.clone()).uri(uri);

        // @TODO: map_err to a RumaResponse and ?
        let path_params: Path<Vec<String>> = Path::<_>::from_request_parts(&mut req, state)
            .await
            .unwrap();
        let mut path_params = path_params.0;
        let any_path = T::METADATA.history.all_paths().next();
        if let Some(path) = any_path {
            let params = path.matches("/:").count();
            for _ in path_params.len()..params {
                path_params.push("".to_string());
            }
        }

        for (k, v) in req.headers.iter() {
            new_request = new_request.header(k.clone(), v.clone());
        }

        // @TODO: map_err to a RumaResponse and ?
        let http_req = new_request.body(body).unwrap();

        let inner = T::try_from_http_request(http_req, &path_params).unwrap();
        Ok(Self(inner))
    }
}

#[async_trait]
impl<S, B, T> FromRequest<S, B> for RumaRequest<T>
where
    S: Send + Sync,
    B: axum::body::HttpBody + Send + Sync + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
    <B as axum::body::HttpBody>::Error: std::fmt::Debug,
    T: IncomingRequest + Send + Sync + std::fmt::Debug,
{
    type Rejection = RumaResponse<ClientError>;

    async fn from_request(req: Request<B>, state: &S) -> Result<Self, Self::Rejection> {
        let (mut parts, body) = req.into_parts();

        let error_missing_body = ClientError {
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
            body: ErrorBody::Standard {
                kind: ErrorKind::Unknown,
                message: "Missing body".to_string(),
            },
        };

        let body = hyper::body::to_bytes(body)
            .await
            .map_err(|_| RumaResponse(error_missing_body.clone()))?
            .to_vec();

        let original_uri = OriginalUri::from_request_parts(&mut parts, state)
            .await
            .map_err(|_| RumaResponse(error_missing_body.clone()))?;

        Self::new(body, &original_uri.0, parts, state)
            .await
            .map_err(RumaResponse)
    }
}

pub struct RumaResponse<T: OutgoingResponse>(pub T);

impl<T: OutgoingResponse> IntoResponse for RumaResponse<T> {
    fn into_response(self) -> Response {
        let mut builder = http::Response::builder();

        match self.0.try_into_http_response::<Vec<u8>>() {
            Ok(response) => {
                for (k, v) in response.headers() {
                    builder = builder.header(k, v);
                }

                let status = response.status();
                let body = response.into_body();

                builder
                    .status(status)
                    .body(axum::body::boxed(axum::body::Full::from(body)))
                    .expect("failed to build response")
            }
            Err(err) => {
                error!(?err, "could not build RumaResponse");
                let error = json!({
                    "errcode": "M_UNKNOWN",
                    "error": "internal server error",
                });

                let raw_body = serde_json::to_string_pretty(&error)
                    .expect("failed to serialize error response");

                builder
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(axum::body::boxed(axum::body::Full::from(raw_body)))
                    .expect("failed to build error response")
            }
        }
    }
}
