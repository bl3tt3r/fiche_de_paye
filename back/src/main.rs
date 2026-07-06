use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Multipart, Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use bitcode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::{debug, info, level_filters::LevelFilter};
use uuid::Uuid;

use crate::{claude::Claude, database::Database, datetime::now};

pub mod claude;
pub mod database;
pub mod datetime;

const DEFAULT_LOG_LEVEL: LevelFilter = tracing::level_filters::LevelFilter::DEBUG;

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

    // Démarrage du cronjob d'analyse des fiches de paie
    spawn_paystub_cron(state.clone());

    // Création des routes
    let mut app = Router::new();
    app = register_status_routes(app);
    app = register_definitions_routes(app);
    app = register_paystub_routes(app);
    let app = app.with_state(state).layer(CorsLayer::permissive());

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
    pub paystub: Arc<Mutex<Database<PaystubState>>>,
}

// ========= STATUS

fn register_status_routes(router: Router<AppState>) -> Router<AppState> {
    router.route("/api/status", get(get_status))
}

#[derive(Serialize)]
pub struct StatusResponse {
    claude_installed: bool,
    claude_connected: bool,
}

pub async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        claude_installed: state.claude.installed(),
        claude_connected: state.claude.connected(),
    })
}

// ========= Paystub

fn register_paystub_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/paystubs", get(get_paystubs))
        .route("/api/paystubs", post(post_paystubs))
        .route("/api/paystubs/{id}/file", get(get_paystub_file))
}

#[derive(Encode, Decode, Serialize)]
pub enum PaystubState {
    Pending(String),
    Analyse(String, u128),
    Analysed(String, Paystub),
}

#[derive(Encode, Decode, Serialize, Deserialize)]
pub struct Paystub {
    infos: HashMap<String, String>,
    datas: HashMap<String, f32>,
}

pub async fn get_paystubs(
    State(state): State<AppState>,
) -> Result<Json<HashMap<String, PaystubState>>, (StatusCode, &'static str)> {
    let paystubs = state.paystub.lock().map_err(|error| {
        let msg = "Verrouillage de la base de données.";
        tracing::error!(cauded = %error, database = "paystub",   msg);
        (StatusCode::INTERNAL_SERVER_ERROR, msg)
    })?;
    let items = paystubs
        .get_all()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(items))
}

pub async fn get_paystub_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    let item = {
        let paystubs = state.paystub.lock().map_err(|error| {
            let msg = "Verrouillage de la base de données.";
            tracing::error!(cauded = %error, database = "paystub", msg);
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        })?;
        paystubs
            .get(&id)
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
    };

    let Some(item) = item else {
        return Err((StatusCode::NOT_FOUND, "Fiche introuvable."));
    };

    let path = match item {
        PaystubState::Pending(path) => path,
        PaystubState::Analyse(path, _) => path,
        PaystubState::Analysed(path, _) => path,
    };

    let bytes = std::fs::read(&path).map_err(|error| {
        let msg = "Lecture du fichier PDF.";
        tracing::error!(cauded = %error, path, msg);
        (StatusCode::INTERNAL_SERVER_ERROR, msg)
    })?;

    Ok((
        [(header::CONTENT_TYPE, "application/pdf")],
        Bytes::from(bytes),
    ))
}

pub async fn post_paystubs(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<HashMap<String, PaystubState>>, (StatusCode, &'static str)> {
    let mut result = HashMap::new();

    while let Some(field) = multipart.next_field().await.map_err(|error| {
        let msg = "Lecture d'un champ multipart.";
        tracing::error!(cauded = %error, msg);
        (StatusCode::INTERNAL_SERVER_ERROR, msg)
    })? {
        let file_name = field.file_name().unwrap_or("inconnu").to_string();
        if !file_name.ends_with(".pdf") {
            let msg = "Seul les PDFs sont authorisés.";
            tracing::warn!(msg);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, msg));
        }
        let data = field.bytes().await.map_err(|error| {
            let msg = "Lecture du contenu du fichier.";
            tracing::error!(cauded = %error, file_name, msg);
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        })?;
        std::fs::create_dir_all("paystubs").map_err(|error| {
            let msg = "Création du dossier paystubs.";
            tracing::error!(cauded = %error, msg);
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        })?;
        let uuid = Uuid::new_v4().to_string();
        let location = format!("paystubs/{uuid}.pdf");
        std::fs::write(&location, &data).map_err(|error| {
            let msg = "Écriture du fichier.";
            tracing::error!(cauded = %error, location, msg);
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        })?;
        let location = std::fs::canonicalize(&location)
            .map_err(|error| {
                let msg = "Résolution du chemin absolu du fichier.";
                tracing::error!(cauded = %error, location, msg);
                (StatusCode::INTERNAL_SERVER_ERROR, msg)
            })?
            .to_string_lossy()
            .to_string();
        let paystubs = state.paystub.lock().map_err(|error| {
            let msg = "Verrouillage de la base de données.";
            tracing::error!(cauded = %error, database = "paystub",   msg);
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        })?;
        paystubs
            .add(&uuid, PaystubState::Pending(location.clone()))
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
        result.insert(uuid, PaystubState::Pending(location));
    }
    Ok(Json(result))
}

// ======== definitions

fn register_definitions_routes(router: Router<AppState>) -> Router<AppState> {
    router.route("/api/definitions", get(get_definitions))
}

pub async fn get_definitions(
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

// ========= Paystubs analyse

const PAYSTUB_CRON_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

const CLAUDE_SYSTEM_PROMPT: &str = r#"
Tu es un assistant IA intégré à une application de gestion de fiches de paie.

MISSION
On te fournit le chemin d'un fichier PDF représentant une fiche de paie. Tu dois le lire et en extraire les données sous forme structurée.

FORMAT DE RÉPONSE
Tu réponds UNIQUEMENT avec un objet JSON valide, sans texte avant ni après, sans balises markdown (pas de ```json```). Le JSON doit respecter exactement ce schéma :

{
  "infos": { "<clé>": "<valeur>" },
  "datas": { "<clé>": "<valeur>" }
}

RÈGLES PAR SECTION
1. "infos" — Date de début/fin de période, employeur, salarié, poste, etc.
2. "datas" — Clé -> valeur pour chaque ligne de calcul. Préfixe la valeur d'un "-" si c'est une charge/déduction.

NOMMAGE DES CLÉS
Toutes les clés doivent être en français, en snake_case, sans accents.

CONTRAINTES GÉNÉRALES
- N'invente aucune donnée absente du document.
- Ignore les données illisibles ou ambiguës.
- Traite toutes les pages du PDF.
- Décimales avec un point ".".
- Aucun commentaire, uniquement le JSON.
- Ne met aucune balise markdown, surtout autour des json.
"#;

/// Lance une tâche de fond qui analyse les fiches de paie en attente toutes les 5 minutes.
fn spawn_paystub_cron(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(PAYSTUB_CRON_INTERVAL);
        loop {
            interval.tick().await;
            if let Err(error) = process_pending_paystubs(&state).await {
                tracing::error!(cauded = %error, "Exécution du cronjob des fiches de paie.");
            }
        }
    });
}

async fn process_pending_paystubs(state: &AppState) -> Result<(), &'static str> {
    // On ne garde le verrou que le temps de choisir une fiche en attente et de
    // marquer son state à `Analyse` : le lock doit être relâché avant l'appel
    // (potentiellement long) à `claude.prompt`, sous peine de bloquer les
    // autres routes de l'API qui ont besoin de cette même base de données.
    let next = {
        let paystubs = state.paystub.lock().map_err(|error| {
            let msg = "Verrouillage de la base de données.";
            tracing::error!(cauded = %error, database = "paystub", msg);
            msg
        })?;

        let pending = paystubs
            .get_all()?
            .into_iter()
            .find(|(_, item)| matches!(item, PaystubState::Pending(_)));

        if let Some((key, PaystubState::Pending(path))) = &pending {
            info!(paystub = key, "Analyse de la fiche de paye.");
            paystubs
                .add(key, PaystubState::Analyse(path.clone(), now()))
                .map_err(|error| {
                    let msg = "Changement de state.";
                    tracing::error!(cauded = %error, paystub = key,  database = "paystub", from = "Pending", to = "Analyse", msg);
                    msg
                })?;
        }

        pending
    };

    let Some((key, PaystubState::Pending(path))) = next else {
        debug!("Aucune fiche de paye a analysée.");
        return Ok(());
    };

    // `claude.prompt` lance un sous-processus et attend sa fin de façon
    // bloquante : on le déporte sur le pool de threads bloquants de tokio
    // pour ne pas geler un worker async pendant toute l'analyse.
    let claude = state.claude.clone();
    let prompt_path = path.clone();
    let result = tokio::task::spawn_blocking(move || {
        claude.prompt(
            CLAUDE_SYSTEM_PROMPT,
            &format!(
                "Lis le fichier \"{}\" et renvoie uniquement le JSON demandé, rien d'autre.",
                prompt_path
            ),
        )
    })
    .await
    .map_err(|error| {
        let msg = "Exécution de la tâche d'analyse.";
        tracing::error!(cauded = %error, paystub = key, msg);
        msg
    })?;

    let paystubs = state.paystub.lock().map_err(|error| {
        let msg = "Verrouillage de la base de données.";
        tracing::error!(cauded = %error, database = "paystub", msg);
        msg
    })?;

    match result {
        Ok(result) => {
            debug!(result = format!("{:#?}", &result), "Réponser de claude.");
            let result = result.result.replace("```json", "").replace("```", "");
            debug!(result, "Analyse de claude.");
            match serde_json::from_str::<Paystub>(&result) {
                Ok(paystub) => {
                    paystubs
                    .add(&key, PaystubState::Analysed(path.clone(), paystub))
                    .map_err(|error| {
                        let msg = "Changement de state.";
                        tracing::error!(cauded = %error, paystub = key,  database = "paystub", from = "Analyse", to = "Analysed", msg);
                        msg
                    })?;
                    info!(paystub = key, "Fiche de paye analysée.");
                }
                Err(error) => {
                    let msg = "Décodage du JSON de la fiche de paye.";
                    tracing::error!(cauded = %error, paystub = key, database = "paystub", msg);
                    paystubs
                    .add(&key, PaystubState::Pending(path.clone()))
                    .map_err(|error| {
                        let msg = "Changement de state.";
                        tracing::error!(cauded = %error, paystub = key,  database = "paystub", from = "Analyse", to = "Pending", msg);
                        msg
                    })?;
                    return Err(msg);
                }
            }
        }
        Err(error) => {
            let msg = "Analyse de la fiche de paye.";
            tracing::error!(caused = error, paystub = key, database = "paystub", msg);
            paystubs
            .add(&key, PaystubState::Pending(path.clone()))
            .map_err(|error| {
                let msg = "Changement de state.";
                tracing::error!(cauded = %error, paystub = key,  database = "paystub", from = "Analyse", to = "Pending", msg);
                msg
            })?;
            return Err(msg);
        }
    }

    Ok(())
}
