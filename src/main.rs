#![feature(absolute_path)]

mod matrix;
mod utils;
mod webhook;

use axum::{
    routing::{get, post, put},
    Router,
};
use eyre::Result as EResult;
use rand::{distributions, Rng};
use ruma::api::appservice::{self, Registration};
use serde::Serialize;
use tracing::*;

use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::{Path, PathBuf};

const APPSERVICE_ID: &'static str = "matrix-free-stuff";
const SENDER_LOCALPART: &'static str = "_freestuff";
const TOKEN_LENGTH: usize = 64;

#[tokio::main]
#[instrument]
async fn main() -> EResult<()> {
    tracing_subscriber::fmt::init();

    let addr = std::env::var("LISTEN_ADDR")
        .map_err(|_| debug!("no address to listen on specified"))
        .unwrap_or("0.0.0.0:3000".to_string());

    let homeserver_url = std::env::var("HOMESERVER_URL")
        .expect("required environment variable HOMESERVER_URL not set");

    let registration: Registration = match std::env::var("APPSERVICE_REGISTRATION")
        .map(PathBuf::from)
        .map(|path| (path.clone(), File::open(&path)))
    {
        Ok((_, Ok(file))) => {
            debug!("loading registration from file");
            serde_yaml::from_reader(file).expect("failed to deserialize registration")
        }
        Ok((path, Err(err))) if err.kind() == std::io::ErrorKind::NotFound => {
            debug!(?err, "failed to open registration file");
            warn!("registration file at path is empty; creating new registration in it's place");

            let absolute_path = std::path::absolute(&path)
                .expect("could not get absolute path of registration file");
            let dir_path = absolute_path.parent().unwrap_or(Path::new("."));

            info!(?dir_path, "creating leading directories");
            if let Err(err) = create_dir_all(dir_path) {
                error!(?err, ?dir_path, "failed to create leading directories");
            }

            match File::create(&absolute_path) {
                Ok(mut file) => {
                    let mut rng = rand::thread_rng();

                    let mut random_string = |length: usize| -> String {
                        (0..length)
                            .map(|_| rng.sample(distributions::Alphanumeric) as char)
                            .collect()
                    };

                    let as_token = random_string(TOKEN_LENGTH);
                    let hs_token = random_string(TOKEN_LENGTH);

                    let mut namespaces = appservice::Namespaces::new();
                    namespaces.users.push(appservice::Namespace::new(true, format!("@{SENDER_LOCALPART}:.*")));

                    let registration: Registration = appservice::RegistrationInit {
                        id: APPSERVICE_ID.to_string(),
                        url: format!("http://{addr}"),
                        as_token,
                        hs_token,
                        sender_localpart: SENDER_LOCALPART.to_string(),
                        namespaces,
                        rate_limited: None,
                        protocols: None,
                    }
                    .into();

                    let serialized = registration
                        .serialize(serde_yaml::value::Serializer)
                        .and_then(|x| serde_yaml::to_string(&x))
                        .expect("failed to serialize registration");

                    match file.write_all(&serialized.into_bytes()) {
                        Ok(_) => info!(?absolute_path, "created registration file"),
                        Err(err) => {
                            error!(?err, ?absolute_path, "failed to write registration file")
                        }
                    }

                    registration.into()
                }
                Err(err) => {
                    error!(?err, ?absolute_path, "could not create registration file");
                    std::process::exit(1);
                }
            }
        }
        Ok((path, Err(err))) => {
            error!(
                ?err,
                ?path,
                "failed to open existing registration file. exiting"
            );
            std::process::exit(1);
        }
        Err(err) => {
            debug!(
                ?err,
                "could not get environment variable APPSERVICE_REGISTRATION"
            );
            error!(
                "no path to registration specified; please set APPSERVICE_REGISTRATION. exiting"
            );
            std::process::exit(1);
        }
    };

    /*
    let client = ruma::client::Client::builder()
        .homeserver_url(homeserver_url)
        .access_token(Some(registration.as_token.clone()))
        .build::<ruma::client::http_client::HyperNativeTls>()
        .await?;
    */

    let webhook_path = std::env::var("WEBHOOK_PATH")
        .map_err(|_| debug!("no webhook path specified"))
        .unwrap_or("/".to_string());
    let webhook_secret = std::env::var("WEBHOOK_SECRET")
        .map(utils::ApiSecret)
        .map_err(|_| warn!("no secret specified"))
        .ok();

    let state = utils::AppState {
        registration,
        ping_transactions: Default::default(),
    };

    let app = Router::new()
        .route(&webhook_path, get(webhook::handle_webhooks))
        .route(&webhook_path, post(webhook::handle_webhooks))
        .with_state(webhook_secret)
        .route("/_matrix/app/v1/ping", post(matrix::handle_ping))
        .route(
            "/_matrix/app/v1/transactions/:transaction_id",
            put(matrix::handle_transactions),
        )
        .with_state(state.clone());

    info!(?addr, "starting webserver");
    axum::Server::bind(&addr.parse()?)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
