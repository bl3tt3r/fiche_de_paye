use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use bitcode::{Decode, Encode};
use serde::Serialize;
use tokio::net::TcpListener;
use tracing::level_filters::LevelFilter;

use crate::{claude::Claude, database::Database};

pub mod claude;
pub mod database;

const DEFAULT_LOG_LEVEL: LevelFilter = tracing::level_filters::LevelFilter::TRACE;

#[tokio::main]
async fn main() {
    // Initialisation du logger
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(DEFAULT_LOG_LEVEL.into())
                .from_env_lossy(),
        )
        .init();

    // Création du state partagé
    let definition = match Database::load("definition") {
        Ok(database) => database,
        Err(error) => {
            tracing::error!(cauded = %error, "Création de la base de données 'definition'.");
            return;
        }
    };
    let paystub = match Database::load("paystub") {
        Ok(database) => database,
        Err(error) => {
            tracing::error!(cauded = %error, "Création de la base de données 'paystub'.");
            return;
        }
    };
    let state = AppState {
        claude: Arc::new(Claude),
        definition: Arc::new(Mutex::new(definition)),
        paystub: Arc::new(Mutex::new(paystub)),
    };

    // Création des routes
    let app = Router::new()
        .route("/api/status", get(status))
        .route("/api/definitions", get(definitions))
        .with_state(state);

    // Création du socket
    let listener = match TcpListener::bind("0.0.0.0:8888").await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(cauded = %error, "Création du socket.");
            return;
        }
    };
    tracing::info!(
        port = listener.local_addr().unwrap().port(),
        "Création du socket."
    );

    // Démarrage du server axum
    if let Err(error) = axum::serve(listener, app).await {
        tracing::error!(cauded = %error, "Demarrage du server.");
    }
}

// ========= AppState

#[derive(Clone)]
pub struct AppState {
    pub claude: Arc<Claude>,
    pub definition: Arc<Mutex<Database<String>>>,
    pub paystub: Arc<Mutex<Database<Paystub>>>,
}

// ========= STATUS

#[derive(Serialize)]
pub struct StatusResponse {
    claude_installed: bool,
    claude_connected: bool,
}

pub async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        claude_installed: state.claude.installed(),
        claude_connected: state.claude.connected(),
    })
}

// ========= Paystub

#[derive(Encode, Decode)]
pub struct Paystub {
    infos: HashMap<String, String>,
    datas: HashMap<String, i32>,
}

pub async fn definitions(
    State(state): State<AppState>,
) -> Result<Json<HashMap<String, String>>, (StatusCode, &'static str)> {
    let definition = state.definition.lock().map_err(|error| {
        let msg = "Verrouillage de la base de données.";
        tracing::error!(cauded = %error, database = "definition",   msg);
        (StatusCode::INTERNAL_SERVER_ERROR, msg)
    })?;
    let items = definition
        .get_all()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(items))
}
