use bitcode::{Decode, Encode};
use redb::{ReadableDatabase, ReadableTable, TableDefinition};
use std::collections::HashMap;
use std::fs;
use std::marker::PhantomData;

const DIR: &str = "datas";
const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("default");

/// Base de données clé-valeur persistante, fondée sur `redb`, dédiée au stockage
/// de valeurs d'un seul type `T`.
pub struct Database<T> {
    name: &'static str,
    db: redb::Database,
    location: String,
    _marker: std::marker::PhantomData<T>,
}

impl<T> Database<T>
where
    T: Encode + for<'a> Decode<'a>,
{
    /// Ouvre le fichier `datas/{name}.redb`, en le créant s'il n'existe pas encore.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use back::database::Database;
    ///
    /// let employes: Database<String> = Database::load("employes")?;
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn load(name: &'static str) -> Result<Self, &'static str> {
        // Création ou ouverture de la base de données sur le disque
        fs::create_dir_all(DIR).map_err(|error| {
            let msg = "Création le dossier datas.";
            tracing::error!(cauded = %error, database = name, msg);
            msg
        })?;
        let location = format!("{}/{}.redb", DIR, name);
        let db = redb::Database::create(&location).map_err(|error| {
            let msg = "Création de la base de données.";
            tracing::error!(cauded = %error, database = name, msg);
            msg
        })?;

        // Création de la table si elle n'existe pas encore
        let transaction = db.begin_write().map_err(|error| {
            let msg = "Ouverture d'une transaction en ecriture.";
            tracing::error!(cauded = %error, database = name, msg);
            msg
        })?;
        transaction.open_table(TABLE).map_err(|error| {
            let msg = "Création de la table.";
            tracing::error!(cauded = %error, database = name, msg);
            msg
        })?;
        transaction.commit().map_err(|error| {
            let msg = "Commit de la transaction.";
            tracing::error!(cauded = %error, database = name, msg);
            msg
        })?;

        Ok(Self {
            name,
            db,
            location,
            _marker: PhantomData,
        })
    }

    /// Insère `value` sous la clé `key`, en remplaçant la valeur existante le cas échéant.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use back::database::Database;
    ///
    /// let mut employes: Database<String> = Database::load("employes")?;
    /// employes.add("42", "Alice Dupont".to_string())?;
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn add(&self, key: &str, value: T) -> Result<(), &'static str> {
        let transaction = self.db.begin_write().map_err(|error| {
            let msg = "Ouverture d'une transaction en ecrite.";
            tracing::error!(cauded = %error, database = self.name,  key, msg);
            msg
        })?;
        {
            let mut table = transaction.open_table(TABLE).map_err(|error| {
                let msg = "Ouverture de la table.";
                tracing::error!(cauded = %error, database = self.name,  key, msg);
                msg
            })?;

            let bytes = bitcode::encode(&value);
            table.insert(key, bytes.as_slice()).map_err(|error| {
                let msg = "Insertion de la valeur.";
                tracing::error!(cauded = %error, database = self.name, key, msg);
                msg
            })?;
        }
        transaction.commit().map_err(|error| {
            let msg = "Commit de la transaction.";
            tracing::error!(cauded = %error, database = self.name,  key, msg);
            msg
        })
    }

    /// Récupère la valeur associée à `key`, ou `None` si la clé est absente.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use back::database::Database;
    ///
    /// let mut employes: Database<String> = Database::load("employes")?;
    /// if let Some(nom) = employes.get("42")? {
    ///     println!("{nom}");
    /// }
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn get(&self, key: &str) -> Result<Option<T>, &'static str> {
        let transaction = self.db.begin_read().map_err(|error| {
            let msg = "Ouverture d'une transaction en lecture.";
            tracing::error!(cauded = %error, database = self.name,  key, msg);
            msg
        })?;
        let table = transaction.open_table(TABLE).map_err(|error| {
            let msg = "Ouverture de la table.";
            tracing::error!(cauded = %error, database = self.name,  key, msg);
            msg
        })?;

        let value = table.get(key).map_err(|error| {
            let msg = "Lecture d'une valeur.";
            tracing::error!(cauded = %error, database = self.name,  key, msg);
            msg
        })?;
        value
            .map(|value| bitcode::decode::<T>(value.value()))
            .transpose()
            .map_err(|error| {
                let msg = "Decodage d'une valeur.";
                tracing::error!(cauded = %error, database = self.name, key, msg);
                msg
            })
    }

    /// Récupère toutes les paires clé/valeur de la table.
    ///
    /// La lecture est atomique : si une entrée est illisible ou corrompue,
    /// l'ensemble de l'opération échoue plutôt que de renvoyer un résultat partiel.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use back::database::Database;
    ///
    /// let mut employes: Database<String> = Database::load("employes")?;
    /// for (id, nom) in employes.get_all()? {
    ///     println!("{id} -> {nom}");
    /// }
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn get_all(&self) -> Result<HashMap<String, T>, &'static str> {
        let transaction = self.db.begin_read().map_err(|error| {
            let msg = "Ouverture d'une transaction en lecture.";
            tracing::error!(cauded = %error, database = self.name, msg);
            msg
        })?;
        let table = transaction.open_table(TABLE).map_err(|error| {
            let msg = "Ouverture de la table.";
            tracing::error!(cauded = %error, database = self.name, msg);
            msg
        })?;
        table
            .iter()
            .map_err(|error| {
                let msg = "Lecture des valeurs.";
                tracing::error!(cauded = %error, database = self.name, msg);
                msg
            })?
            .map(|entry| {
                let (key, value) = entry.map_err(|error| {
                    let msg = "Lecture d'une entrée.";
                    tracing::error!(cauded = %error, database = self.name, msg);
                    msg
                })?;
                let key = key.value().to_string();
                bitcode::decode::<T>(value.value())
                    .map(|v| (key.clone(), v))
                    .map_err(|error| {
                        let msg = "Décodage d'une valeur.";
                        tracing::error!(cauded = %error, database = self.name, key, msg);
                        msg
                    })
            })
            .collect()
    }

    /// Supprime le fichier `.redb` associé à cette base de données.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use back::database::Database;
    ///
    /// let employes: Database<String> = Database::load("employes")?;
    /// employes.delete()?;
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn delete(self) -> Result<(), &'static str> {
        fs::remove_file(&self.location).map_err(|error| {
            let msg = "Suppression de la base de données.";
            tracing::error!(cauded = %error, database = self.name, location = self.location, msg);
            msg
        })
    }

    /// Supprime la valeur associée à `key`, sans effet si la clé est absente.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use back::database::Database;
    ///
    /// let mut employes: Database<String> = Database::load("employes")?;
    /// employes.remove("42")?;
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn remove(&mut self, key: &str) -> Result<(), &'static str> {
        let transaction = self.db.begin_write().map_err(|error| {
            let msg = "Ouverture d'une transaction en écriture.";
            tracing::error!(cauded = %error, database = self.name, key, msg);
            msg
        })?;
        {
            let mut table = transaction.open_table(TABLE).map_err(|error| {
                let msg = "Ouverture de la table.";
                tracing::error!(cauded = %error, database = self.name, key, msg);
                msg
            })?;

            table.remove(key).map_err(|error| {
                let msg = "Suppression de la valeur.";
                tracing::error!(cauded = %error, database = self.name, key, msg);
                msg
            })?;
        }
        transaction.commit().map_err(|error| {
            let msg = "Commit de la transaction.";
            tracing::error!(cauded = %error, database = self.name, key, msg);
            msg
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn add_puis_get_renvoie_la_valeur() {
        let mut database: Database<String> = Database::load("test_add_puis_get").unwrap();

        let value = database
            .add("cle", "valeur".to_string())
            .and_then(|_| database.get("cle"));

        database.delete().unwrap();
        assert_eq!(value.unwrap(), Some("valeur".to_string()));
    }

    #[test_log::test]
    fn get_sur_cle_absente_renvoie_none() {
        let mut database: Database<String> = Database::load("test_get_cle_absente").unwrap();

        let value = database
            .add("autre_cle", "valeur".to_string())
            .and_then(|_| database.get("inconnue"));

        database.delete().unwrap();
        assert_eq!(value.unwrap(), None);
    }

    #[test_log::test]
    fn add_ecrase_la_valeur_existante() {
        let mut database: Database<String> = Database::load("test_add_ecrase").unwrap();

        let value = database
            .add("cle", "premiere".to_string())
            .and_then(|_| database.add("cle", "seconde".to_string()))
            .and_then(|_| database.get("cle"));

        database.delete().unwrap();
        assert_eq!(value.unwrap(), Some("seconde".to_string()));
    }

    #[test_log::test]
    fn remove_supprime_la_valeur() {
        let mut database: Database<String> = Database::load("test_remove").unwrap();

        let value = database
            .add("cle", "valeur".to_string())
            .and_then(|_| database.remove("cle"))
            .and_then(|_| database.get("cle"));

        database.delete().unwrap();
        assert_eq!(value.unwrap(), None);
    }

    #[test_log::test]
    fn remove_sur_cle_absente_ne_renvoie_pas_derreur() {
        let mut database: Database<String> = Database::load("test_remove_absente").unwrap();

        let value = database.remove("inconnue");

        database.delete().unwrap();
        value.unwrap();
    }

    #[test_log::test]
    fn get_all_renvoie_toutes_les_entrees() {
        let mut database: Database<String> = Database::load("test_get_all").unwrap();

        let all = database
            .add("a", "1".to_string())
            .and_then(|_| database.add("b", "2".to_string()))
            .and_then(|_| database.get_all());

        database.delete().unwrap();
        let all = all.unwrap();

        assert_eq!(all.len(), 2);
        assert_eq!(all.get("a"), Some(&"1".to_string()));
        assert_eq!(all.get("b"), Some(&"2".to_string()));
    }

    #[test_log::test]
    fn get_all_renvoie_une_map_vide() {
        let database: Database<String> = Database::load("test_get_all_empty").unwrap();
        let all = database.get_all();
        database.delete().unwrap();
        let all = all.unwrap();
        assert_eq!(all.len(), 0);
    }
}
