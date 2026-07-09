use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};

use duct::cmd;
use eframe::egui::Context;
use serde::Deserialize;
use time::Date;
use time::macros::format_description;

use crate::app::paystubs::{Paystub, PaystubState};

const CLAUDE_SYSTEM_PROMPT: &str = r#"
Tu es un assistant IA intégré à une application de gestion de fiches de paie.

## MISSION
On te fournit le chemin d'un fichier PDF représentant une fiche de paie (une ou plusieurs pages).
Tu dois lire l'intégralité du document et en extraire les données sous forme structurée, fidèle au contenu réel du document.

## FORMAT DE SORTIE
Tu réponds UNIQUEMENT avec un objet JSON valide, strictement conforme au schéma ci-dessous.
- Pas de texte avant ou après le JSON.
- Pas de balises markdown (pas de ```json``` ni de ``` seuls).
- Pas de commentaires, pas de virgule finale, JSON strictement valide.

Schéma exact :
{
  "payment_date": "<AAAA-MM-JJ>",
  "net_salary": <valeur number>,
  "infos": { "<clé>": "<valeur string>" },
  "datas": { "<clé>": <valeur number> }
}

## RÈGLES PAR SECTION

### "payment_date" et "net_salary" (obligatoires)
En plus de "infos" et "datas", tu dois toujours renvoyer ces deux champs de premier niveau : ce sont deux données présentes sur toute fiche de paie, et l'application en a besoin sous une forme fixe (elle ne cherche pas ces valeurs dans "infos"/"datas").
- "payment_date" : la date de paiement/versement de la fiche, au format "AAAA-MM-JJ" (ISO 8601), en chaîne de caractères. Si cette date n'apparaît pas explicitement dans le contenu du document, tu peux la déduire du nom du fichier PDF fourni (le nom de fichier contient souvent une date ou une période).
- "net_salary" : le montant du salaire net (net à payer) de la fiche, au même format que les valeurs de "datas" (nombre JSON natif, point décimal, jamais de chaîne de caractères).

Ces deux champs sont indépendants de "infos" et "datas" : ne les omets jamais, même si la même information y apparaît déjà sous une autre forme ou un autre libellé.

### "infos"
Informations d'identification et de contexte : période (date de début / date de fin), employeur, salarié, poste, et tout autre champ d'identification présent sur le document.
Les valeurs sont des chaînes de caractères brutes, sans transformation ni calcul.

### "datas"
Une entrée par ligne de calcul de la fiche de paie (cotisations, salaire brut, net, primes, retenues, etc.).
- La clé est le libellé exact de la ligne (voir NOMMAGE DES CLÉS).
- La valeur est un **nombre JSON natif (float)**, jamais une chaîne de caractères. Pas de guillemets, pas d'espace, pas de séparateur de milliers.
- Utilise toujours le point "." comme séparateur décimal (ex. 1234.56).
- Si la ligne correspond à une charge, cotisation ou déduction (montant retranché du total), la valeur doit être négative (ex. -123.45).
- Les montants positifs ou neutres restent positifs, sans signe "+".
- Cette règle de signe ne s'applique qu'à "datas", jamais à "infos".

## NOMMAGE DES CLÉS
Chaque clé doit reproduire exactement le libellé tel qu'il apparaît sur la fiche de paie (casse, accents, ponctuation), sans reformulation, traduction ni abréviation.
Si un même libellé apparaît plusieurs fois avec des valeurs différentes (ex. sur plusieurs pages), distingue-les par un suffixe clair plutôt que d'écraser une valeur (ex. "Libellé (2)").

## CONTRAINTES GÉNÉRALES
- N'invente aucune donnée absente du document.
- Ignore une donnée si elle est illisible, ambiguë, ou si son interprétation n'est pas certaine — ne l'inclus simplement pas dans le JSON. Cette tolérance ne s'applique pas à "payment_date"/"net_salary", qui doivent toujours être renseignés du mieux possible.
- Traite l'ensemble des pages du PDF, pas seulement la première.
- Un document sans aucune donnée exploitable renvoie tout de même le schéma complet, avec des objets "infos" et "datas" vides ({}).
- Toute valeur dans "datas" doit être un nombre valide au sens JSON (pas de "N/A", pas de chaîne vide, pas de texte) : si la valeur numérique n'est pas certaine, omets la clé plutôt que d'y mettre une valeur non numérique.

## GESTION D'UNE ERREUR PRÉCÉDENTE
Il est possible qu'en plus du PDF, on te fournisse le message d'erreur d'une exécution précédente de ce même prompt ayant échoué (ex. JSON invalide, format non respecté, valeur non numérique dans "datas").
Si un tel message est fourni :
- Analyse-le pour comprendre la cause de l'échec.
- Corrige ta réponse pour ne pas reproduire la même erreur.
- Ne mentionne jamais cette erreur ni son analyse dans ta réponse : le résultat final reste uniquement le JSON attendu.
"#;

/// Formats essayés pour parser `ClaudeAnalysis::payment_date`. Le prompt
/// système impose "AAAA-MM-JJ", mais on garde un format de repli au cas où
/// claude s'en écarte malgré la consigne.
const PAYMENT_DATE_FORMATS: &[&[time::format_description::FormatItem]] = &[
    format_description!("[year]-[month]-[day]"),
    format_description!("[day]/[month]/[year]"),
];

fn parse_payment_date(value: &str) -> Option<i32> {
    let value = value.trim();
    PAYMENT_DATE_FORMATS
        .iter()
        .find_map(|format| Date::parse(value, format).ok())
        .map(Date::to_julian_day)
}

/// Pilote l'analyse des fiches de paie sur un thread dédié, pour ne jamais
/// bloquer l'UI egui pendant l'appel (potentiellement long) à `claude`.
///
/// On transfère des `Paystub` dans les deux sens : à l'aller pour disposer,
/// en cas de retry (`ProcessingError`), de l'erreur précédente à transmettre
/// à claude ; au retour pour renvoyer directement le nouvel état (`Completed`
/// ou `ProcessingError`) au caller, qui n'a plus qu'à le persister.
pub struct Analyse {
    sender: Sender<(String, Paystub)>,
    receiver: Receiver<(String, Paystub)>,
    _worker: std::thread::JoinHandle<()>,
}

impl Analyse {
    /// `ctx` est le `Context` egui de l'appli : c'est ce qui permet au
    /// thread de fond de réveiller la fenêtre (`request_repaint`) quand un
    /// résultat arrive. Sans ça, l'UI ne redessine que sur interaction
    /// utilisateur (bouger la souris, cliquer...) et un résultat arrivé
    /// pendant que rien ne bouge resterait invisible jusqu'à la prochaine
    /// interaction.
    pub fn new(ctx: Context) -> Self {
        let (sender, request_rx) = std::sync::mpsc::channel::<(String, Paystub)>();
        let (response_tx, receiver) = std::sync::mpsc::channel::<(String, Paystub)>();

        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("construction du runtime tokio d'analyse");

            tracing::info!("thread d'analyse démarré");

            runtime.block_on(async move {
                // Une seule fiche est reçue à la fois, mais son analyse est
                // déléguée à `spawn_blocking` : la boucle peut donc repartir
                // sur `recv()` et traiter plusieurs fiches en parallèle sans
                // attendre la fin de chaque appel à `claude`.
                while let Ok((id, paystub)) = request_rx.recv() {
                    tracing::debug!(id, file = %paystub.file, "fiche reçue par le thread d'analyse");
                    let response_tx = response_tx.clone();
                    let ctx = ctx.clone();
                    tokio::task::spawn_blocking(move || {
                        let result = analyse_paystub(&id, paystub);
                        let _ = response_tx.send((id, result));
                        ctx.request_repaint();
                    });
                }
            });

            tracing::info!("thread d'analyse arrêté (canal d'envoi fermé)");
        });

        Self {
            sender,
            receiver,
            _worker: worker,
        }
    }

    /// Envoie une fiche à analyser au thread de fond. `id` est la clé de la
    /// fiche dans `Store::paystubs`, renvoyée telle quelle par `try_recv`
    /// pour que le résultat puisse être réinjecté au bon endroit.
    pub fn analyse(&self, id: String, paystub: Paystub) {
        tracing::debug!(id, file = %paystub.file, "envoi d'une fiche au thread d'analyse");
        // Échoue seulement si le thread de fond s'est arrêté (panic) ; rien
        // à faire de plus ici, l'appelant ne recevra simplement pas de réponse.
        if self.sender.send((id, paystub)).is_err() {
            tracing::error!("le thread d'analyse est arrêté, impossible d'envoyer la fiche");
        }
    }

    /// Récupère un résultat d'analyse disponible, sans bloquer.
    ///
    /// À appeler à chaque frame (ex. dans `Components::update`) pour
    /// consommer les résultats au fur et à mesure qu'ils arrivent : la
    /// fiche renvoyée est déjà à l'état `Completed` ou `ProcessingError`.
    pub fn try_recv(&self) -> Option<(String, Paystub)> {
        self.receiver.try_recv().ok()
    }
}

/// Analyse une fiche et renvoie son nouvel état (`Completed` en cas de
/// succès, `ProcessingError` sinon) ; ne renvoie jamais la fiche inchangée.
/// `id` ne sert qu'à contextualiser les logs (voir `Store::paystubs`).
fn analyse_paystub(id: &str, paystub: Paystub) -> Paystub {
    // `to_completed`/`to_processing_error` n'acceptent pas `Pending` comme
    // état de départ : on passe d'abord par `Processing`. Un retry est déjà
    // dans un état accepté (`ProcessingError`) et reste tel quel.
    let paystub = paystub.to_processing().unwrap_or(paystub);

    let previous_error = match &paystub.state {
        PaystubState::ProcessingError { error, .. } => Some(error.as_str()),
        _ => None,
    };

    tracing::info!(
        id,
        file = %paystub.file,
        retry = previous_error.is_some(),
        "analyse de la fiche démarrée"
    );

    match analyse_file(&paystub.file, previous_error) {
        Ok(outcome) => {
            tracing::info!(
                id,
                file = %paystub.file,
                payment_date = outcome.payment_date,
                net_salary = outcome.net_salary,
                infos = outcome.infos.len(),
                datas = outcome.datas.len(),
                "analyse terminée avec succès"
            );
            paystub
                .to_completed(
                    outcome.payment_date,
                    outcome.net_salary,
                    outcome.infos,
                    outcome.datas,
                )
                .unwrap_or(paystub)
        }
        Err(error) => {
            tracing::warn!(id, file = %paystub.file, %error, "échec de l'analyse");
            paystub.to_processing_error(&error).unwrap_or(paystub)
        }
    }
}

fn analyse_file(file: &str, previous_error: Option<&str>) -> Result<AnalysisOutcome, String> {
    let claude = Claude;

    let user_prompt = match previous_error {
        Some(error) => format!("Fichier: {file}\n\nErreur précédente:\n{error}"),
        None => file.to_string(),
    };

    tracing::debug!(file, "appel de claude");
    let response = claude.prompt(CLAUDE_SYSTEM_PROMPT, &user_prompt)?;
    tracing::debug!(
        file,
        duration_ms = response.duration_ms,
        num_turns = response.num_turns,
        "claude a répondu"
    );

    if response.is_error {
        tracing::warn!(
            file,
            api_error_status = ?response.api_error_status,
            "claude a renvoyé une erreur"
        );
        return Err(response.result);
    }

    let cleaned = sanitize_json_response(&response.result);
    let analysis: ClaudeAnalysis = serde_json::from_str(cleaned).map_err(|error| {
        tracing::warn!(file, %error, response = %response.result, "réponse de claude non parsable en JSON");
        format!("Réponse de claude invalide: {error}")
    })?;

    let payment_date = parse_payment_date(&analysis.payment_date).ok_or_else(|| {
        tracing::warn!(
            file,
            payment_date = analysis.payment_date,
            "date de paiement renvoyée par claude non parsable"
        );
        format!(
            "Date de paiement invalide reçue de claude: \"{}\" (format attendu: AAAA-MM-JJ)",
            analysis.payment_date
        )
    })?;

    Ok(AnalysisOutcome {
        payment_date,
        net_salary: analysis.net_salary,
        infos: analysis.infos,
        datas: analysis.datas,
    })
}

/// Nettoie une réponse de claude avant de la parser en JSON.
///
/// Malgré la consigne du prompt système, il arrive que la réponse soit
/// entourée d'une balise markdown (` ```json `/` ``` `) ou de texte parasite
/// avant/après l'objet JSON. On retire ces balises puis, par sécurité, on se
/// limite au premier objet JSON complet trouvé (du premier `{` au dernier `}`).
fn sanitize_json_response(raw: &str) -> &str {
    let mut s = raw.trim();
    let mut had_fence = false;

    if let Some(rest) = s.strip_prefix("```json") {
        s = rest;
        had_fence = true;
    } else if let Some(rest) = s.strip_prefix("```") {
        s = rest;
        had_fence = true;
    }

    if let Some(rest) = s.strip_suffix("```") {
        s = rest;
        had_fence = true;
    }

    s = s.trim();

    if had_fence {
        tracing::debug!("réponse de claude nettoyée (balises markdown détectées)");
    }

    match (s.find('{'), s.rfind('}')) {
        (Some(start), Some(end)) if start <= end => &s[start..=end],
        _ => s,
    }
}

#[derive(Deserialize, Debug)]
struct ClaudeAnalysis {
    payment_date: String,
    net_salary: f32,
    infos: HashMap<String, String>,
    datas: HashMap<String, f32>,
}

/// Résultat d'une analyse réussie : voir `Paystub::to_completed`.
struct AnalysisOutcome {
    payment_date: i32,
    net_salary: f32,
    infos: HashMap<String, String>,
    datas: HashMap<String, f32>,
}

struct Claude;

#[derive(Deserialize, Debug)]
struct ClaudeResponse {
    pub r#type: String,
    pub subtype: String,
    pub is_error: bool,
    pub api_error_status: Option<String>,
    pub duration_ms: i32,
    pub duration_api_ms: i32,
    pub ttft_ms: i32,
    pub ttft_stream_ms: i32,
    pub time_to_request_ms: i32,
    pub num_turns: i32,
    pub result: String,
}

impl Claude {
    /* pub fn installed(&self) -> bool {
        let (code, _stdout, _stderr) = exec(vec!["--version"]);
        code == 0
    }

    pub fn connected(&self) -> bool {
        let (code, stdout, _stderr) = exec(vec!["auth", "status"]);
        code == 0 && stdout.contains("\"loggedIn\": true")
    } */

    pub fn prompt(
        &self,
        system_prompt: &'static str,
        user_prompt: &str,
    ) -> Result<ClaudeResponse, String> {
        tracing::debug!(prompt_len = user_prompt.len(), "invocation de claude");

        let (code, stdout, stderr) = exec(vec![
            "-p",
            "--system-prompt",
            system_prompt,
            "--allowedTools",
            "Read",
            "--output-format",
            "json",
            user_prompt,
        ]);

        if code != 0 {
            tracing::warn!(code, %stderr, "claude a terminé en erreur");
            return Err(stderr.clone());
        }

        Ok(
            serde_json::from_str::<ClaudeResponse>(&stdout).map_err(|error| {
                let msg = "Lecture de la réponse de claude code.";
                tracing::error!(cauded = %error,  msg);
                msg
            })?,
        )
    }
}

fn exec(args: Vec<&str>) -> (i32, String, String) {
    tracing::trace!(command = %format!("claude {}", args.join(" ")), "exécution d'une commande claude");

    let process = cmd("claude", args.clone())
        .stdout_capture()
        .stderr_capture();
    let result = process.run();
    match result {
        Ok(result) => (
            result.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&result.stdout).to_string(),
            String::from_utf8_lossy(&result.stderr).to_string(),
        ),
        Err(error) => {
            let msg = "Execution d'une commande.";
            tracing::warn!(cauded = %error, command = format!("claude {}", args.join(" ")),  msg);
            (-1, "".to_string(), "".to_string())
        }
    }
}
