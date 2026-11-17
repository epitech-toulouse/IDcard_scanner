use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Erreur base de données : {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Erreur fichier : {0}")]
    Io(#[from] std::io::Error),
    #[error("Erreur CSV : {0}")]
    Csv(#[from] csv::Error),
    #[error("Erreur lecture Excel : {0}")]
    Calamine(#[from] calamine::Error),
    #[error("Erreur écriture Excel : {0}")]
    Xlsx(#[from] rust_xlsxwriter::XlsxError),
    #[error("{0} introuvable")]
    NotFound(&'static str),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Sérialisé en simple message pour le front (Tauri renvoie l'erreur au JS).
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// Convertit une violation de contrainte UNIQUE en message lisible.
pub(crate) fn unique_violation(e: rusqlite::Error, what: &str) -> Error {
    if let rusqlite::Error::SqliteFailure(f, _) = &e {
        if f.code == rusqlite::ErrorCode::ConstraintViolation {
            return Error::Invalid(format!("{what} existe déjà"));
        }
    }
    Error::Db(e)
}
