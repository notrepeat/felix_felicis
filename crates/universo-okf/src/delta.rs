use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use universo_core::{Atomo, Delta, ErrorDelta};

pub use universo_core::LadoDelta;

const SCHEMA_V1: &str = "felix-delta-v1";

pub enum ErrorDeltaJson {
    Json,
    SchemaNoSoportado,
    AtomoInvalido { lado: LadoDelta, indice: usize },
    AtomoNoCanonico { lado: LadoDelta, indice: usize },
    AtomoDuplicado { lado: LadoDelta, indice: usize },
    Solapamiento,
}

impl std::fmt::Display for ErrorDeltaJson {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorDeltaJson::Json => write!(f, "delta JSON inválido"),
            ErrorDeltaJson::SchemaNoSoportado => write!(f, "schema de delta no soportado"),
            ErrorDeltaJson::AtomoInvalido { lado, indice } => {
                write!(f, "átomo inválido en {lado}[{indice}]")
            }
            ErrorDeltaJson::AtomoNoCanonico { lado, indice } => {
                write!(f, "átomo no canónico en {lado}[{indice}]")
            }
            ErrorDeltaJson::AtomoDuplicado { lado, indice } => {
                write!(f, "átomo duplicado en {lado}[{indice}]")
            }
            ErrorDeltaJson::Solapamiento => write!(f, "add y remove se solapan"),
        }
    }
}

impl std::fmt::Debug for ErrorDeltaJson {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

impl std::error::Error for ErrorDeltaJson {}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DocumentoDeltaV1 {
    schema: String,
    add: Vec<String>,
    remove: Vec<String>,
}

/// Parsea el contrato JSON v1 y valida texto atómico canónico, duplicados y solapamiento.
pub fn parsear_delta_json_v1(texto: &str) -> Result<Delta, ErrorDeltaJson> {
    let documento: DocumentoDeltaV1 =
        serde_json::from_str(texto).map_err(|_| ErrorDeltaJson::Json)?;
    if documento.schema != SCHEMA_V1 {
        return Err(ErrorDeltaJson::SchemaNoSoportado);
    }

    let add = parsear_lista(documento.add, LadoDelta::Add)?;
    let remove = parsear_lista(documento.remove, LadoDelta::Remove)?;
    Delta::nuevo(add, remove).map_err(|error| match error {
        ErrorDelta::Solapamiento => ErrorDeltaJson::Solapamiento,
        ErrorDelta::AtomoNoRepresentable { lado, indice } => {
            ErrorDeltaJson::AtomoNoCanonico { lado, indice }
        }
    })
}

fn parsear_lista(textos: Vec<String>, lado: LadoDelta) -> Result<Vec<Atomo>, ErrorDeltaJson> {
    let mut atomos = Vec::with_capacity(textos.len());
    let mut vistos = HashSet::with_capacity(textos.len());
    for (indice, texto) in textos.into_iter().enumerate() {
        let atomo =
            Atomo::parsear(&texto).map_err(|_| ErrorDeltaJson::AtomoInvalido { lado, indice })?;
        if atomo.to_string() != texto {
            return Err(ErrorDeltaJson::AtomoNoCanonico { lado, indice });
        }
        if !vistos.insert(atomo.clone()) {
            return Err(ErrorDeltaJson::AtomoDuplicado { lado, indice });
        }
        atomos.push(atomo);
    }
    Ok(atomos)
}

/// Serializa el delta en JSON compacto y determinista, con átomos en orden canónico.
pub fn serializar_delta_json_v1(delta: &Delta) -> Result<String, ErrorDeltaJson> {
    let documento = DocumentoDeltaV1 {
        schema: SCHEMA_V1.to_string(),
        add: delta.agregar().iter().map(ToString::to_string).collect(),
        remove: delta.retirar().iter().map(ToString::to_string).collect(),
    };
    serde_json::to_string(&documento).map_err(|_| ErrorDeltaJson::Json)
}
