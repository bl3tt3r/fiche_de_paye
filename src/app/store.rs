use crate::app::{DATA_DIR, paystubs::Paystub, theme::Theme};
use bitcode::{Decode, Encode};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Read, Write},
};

pub const STORE_FILE: &str = "store.d";

#[derive(Encode, Decode, Default)]
pub struct Store {
    pub theme: Theme,
    /// Clé par un UUID (v4, stocké en `String`) attribué à la création de la
    /// fiche : un identifiant stable, indépendant du `file` (qui peut changer
    /// si le fichier est renommé/déplacé), et qui permet un lookup en O(1).
    pub paystubs: HashMap<String, Paystub>,
}

impl Store {
    pub fn load() -> Result<Store, &'static str> {
        // Création du dossier DATA_DIR s'il n'existe pas.
        fs::create_dir_all(DATA_DIR).map_err(|error| {
            let msg = "Création du dossier datas.";
            tracing::error!(caused = %error,  msg);
            msg
        })?;
        // Création/Ouverture du fichier de store
        let mut store = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true) // crée si absent, ne touche pas si présent
            .open(format!("{}/{}", DATA_DIR, STORE_FILE))
            .map_err(|error| {
                let msg = "Chargement du fichier de store.";
                tracing::error!(caused = %error,  msg);
                msg
            })?;
        // Lecture du contenue du fichier de store
        let mut content = Vec::new();
        store.read_to_end(&mut content).map_err(|error| {
            let msg = "Lecture du fichier de store.";
            tracing::error!(caused = %error,  msg);
            msg
        })?;
        // Fichier fraichement créé (encore vide) : store par défaut.
        if content.is_empty() {
            return Ok(Store::default());
        }

        // Décodage du contenue du fichier de store
        bitcode::decode::<Store>(&content).map_err(|error| {
            let msg = "Décodage du fichier de store.";
            tracing::error!(caused = %error, msg);
            msg
        })
    }

    pub fn save(&self) {
        if let Err(error) = fs::create_dir_all(DATA_DIR) {
            tracing::error!(caused = %error, "Création du dossier datas.");
            return;
        }

        let file = match File::create(format!("{}/{}", DATA_DIR, STORE_FILE)) {
            Ok(file) => file,
            Err(error) => {
                tracing::error!(caused = %error, "Ouverture du fichier de store.");
                return;
            }
        };

        let content = bitcode::encode(self);
        let mut writer = BufWriter::new(file);
        if let Err(error) = writer.write_all(&content) {
            tracing::error!(caused = %error, "Sauvegarde du fichier de store.");
        }
    }
}
