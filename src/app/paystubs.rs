use bitcode::{Decode, Encode};
use std::collections::HashMap;
use time::Timestamp;

type DateTime = i64;

/// Délai au-delà duquel une fiche restée en `Processing` est considérée
/// bloquée (ex: l'application a crashé/redémarré pendant l'analyse) : voir
/// `Paystub::is_stuck`.
pub const ANALYSE_TIMEOUT_MS: DateTime = 10 * 60 * 1000;

/// Erreurs pouvant survenir sur un `Paystub` ou son stockage.
#[derive(Debug)]
pub enum PaystubError {
    /// Transition d'état demandée non autorisée depuis l'état courant.
    Transition {
        from: &'static str,
        to: &'static str,
    },
    /// Échec de lecture/écriture dans la `Database` sous-jacente ; contient
    /// le message d'erreur générique renvoyé par `Database`.
    Database(&'static str),
}

/// Fiche de paie en cours de traitement.
///
/// `file` et `since` sont communs à tous les états, d'où leur présence sur la
/// struct plutôt que dupliqués dans chaque variante de `PaystubState`.
/// `since` n'est pas la date de création de la fiche : c'est l'horodatage
/// d'entrée dans l'état courant, recalculé à chaque transition (voir les
/// méthodes `to_*`).
#[derive(Encode, Decode, Clone)]
pub struct Paystub {
    pub file: String,
    pub since: DateTime,
    pub state: PaystubState,
}

/// État d'une fiche de paie.
///
/// Chaque variante ne porte que les données propres à son état (ex:
/// `error`/`retry` uniquement pour `ProcessingError`). Les transitions entre
/// états se font via les méthodes `to_*` de `impl Paystub`, jamais en
/// construisant directement un `Paystub` depuis l'extérieur (hormis
/// `Pending` via `Paystub::pending`).
#[derive(Encode, Decode, Clone)]
pub enum PaystubState {
    /// Fichier détecté, en attente de traitement.
    Pending,
    /// Traitement du fichier en cours.
    Processing,
    /// Le traitement a échoué ; `retry` compte le nombre de tentatives.
    ProcessingError { error: String, retry: u8 },
    /// Traitement terminé avec succès ; `datas` contient les valeurs extraites.
    Completed {
        payment_date: i32,
        net_salary: f32,
        infos: HashMap<String, String>,
        datas: HashMap<String, f32>,
    },
}

impl Paystub {
    /// Crée une nouvelle fiche de paie à l'état `Pending`, avec l'horodatage courant.
    pub fn pending(file: String) -> Paystub {
        Paystub {
            file,
            since: Timestamp::now().as_milliseconds(),
            state: PaystubState::Pending,
        }
    }

    /// Passe la fiche à l'état `Processing`.
    ///
    /// Seule une fiche `Pending` peut effectuer cette transition ; tout
    /// autre état renvoie `PaystubError::Transition`.
    pub fn to_processing(&self) -> Result<Paystub, PaystubError> {
        match &self.state {
            PaystubState::Pending => Ok(Paystub {
                file: self.file.clone(),
                since: Timestamp::now().as_milliseconds(),
                state: PaystubState::Processing,
            }),
            PaystubState::Processing => Err(PaystubError::Transition {
                from: "Processing",
                to: "Processing",
            }),
            PaystubState::ProcessingError { .. } => Err(PaystubError::Transition {
                from: "ProcessingError",
                to: "Processing",
            }),
            PaystubState::Completed { .. } => Err(PaystubError::Transition {
                from: "Completed",
                to: "Processing",
            }),
        }
    }

    /// Passe la fiche à l'état `ProcessingError`.
    ///
    /// Autorisé depuis `Processing` (première erreur, `retry = 1`) et depuis
    /// `ProcessingError` (nouvel essai, `retry` incrémenté). Tout autre état
    /// renvoie `PaystubError::Transition`.
    pub fn to_processing_error(&self, error: &str) -> Result<Paystub, PaystubError> {
        match &self.state {
            PaystubState::Pending => Err(PaystubError::Transition {
                from: "Pending",
                to: "ProcessingError",
            }),
            PaystubState::Processing => Ok(Paystub {
                file: self.file.clone(),
                since: Timestamp::now().as_milliseconds(),
                state: PaystubState::ProcessingError {
                    error: error.to_string(),
                    retry: 1,
                },
            }),
            PaystubState::ProcessingError { retry, .. } => Ok(Paystub {
                file: self.file.clone(),
                since: Timestamp::now().as_milliseconds(),
                state: PaystubState::ProcessingError {
                    error: error.to_string(),
                    retry: retry + 1,
                },
            }),
            PaystubState::Completed { .. } => Err(PaystubError::Transition {
                from: "Completed",
                to: "ProcessingError",
            }),
        }
    }

    /// Passe la fiche à l'état `Completed` avec les données extraites.
    ///
    /// Autorisé depuis `Processing` ou `ProcessingError` (un retry réussi
    /// aboutit directement à `Completed`). Tout autre état renvoie
    /// `PaystubError::Transition`.
    pub fn to_completed(
        &self,
        payment_date: i32,
        net_salary: f32,
        infos: HashMap<String, String>,
        datas: HashMap<String, f32>,
    ) -> Result<Paystub, PaystubError> {
        match &self.state {
            PaystubState::Pending => Err(PaystubError::Transition {
                from: "Pending",
                to: "Completed",
            }),
            PaystubState::Processing | PaystubState::ProcessingError { .. } => Ok(Paystub {
                file: self.file.clone(),
                since: Timestamp::now().as_milliseconds(),
                state: PaystubState::Completed {
                    payment_date,
                    net_salary,
                    infos,
                    datas,
                },
            }),
            PaystubState::Completed { .. } => Err(PaystubError::Transition {
                from: "Completed",
                to: "Completed",
            }),
        }
    }

    /// `true` si la fiche est en `Processing` depuis plus de
    /// `ANALYSE_TIMEOUT_MS` : plus aucun traitement en cours n'est censé
    /// prendre aussi longtemps, donc soit il est réellement bloqué, soit
    /// l'application qui le pilotait a redémarré entre-temps (ex: crash).
    pub fn is_stuck(&self) -> bool {
        matches!(self.state, PaystubState::Processing)
            && Timestamp::now().as_milliseconds() - self.since > ANALYSE_TIMEOUT_MS
    }

    /// Fait repasser une fiche bloquée (voir `is_stuck`) en `ProcessingError`
    /// avec un message explicite. Ce message est destiné à être transmis à
    /// claude comme "erreur précédente" (voir la section "GESTION D'UNE
    /// ERREUR PRÉCÉDENTE" du prompt système) : ce n'est pas un problème de
    /// format à corriger, juste une analyse interrompue à recommencer.
    pub fn to_timed_out(&self) -> Result<Paystub, PaystubError> {
        self.to_processing_error(
            "Timeout : l'analyse précédente de cette fiche a été interrompue avant de \
             produire un résultat (plus de 10 minutes sans réponse, probablement à cause \
             d'un redémarrage de l'application pendant le traitement). Il ne s'agit pas \
             d'une erreur de format : relance simplement une analyse complète du document.",
        )
    }
}
