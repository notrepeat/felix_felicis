use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use universo_core::{Atomo, BundlePath, Delta, ErrorDelta, Etiqueta, NotaNueva, Sorte};

pub use universo_core::LadoDelta;

const SCHEMA_V1: &str = "felix-delta-v1";
const SCHEMA_V2: &str = "felix-delta-v2";

pub enum ErrorDeltaJson {
    Json,
    SchemaNoSoportado,
    AtomoInvalido {
        lado: LadoDelta,
        indice: usize,
    },
    AtomoNoCanonico {
        lado: LadoDelta,
        indice: usize,
    },
    AtomoDuplicado {
        lado: LadoDelta,
        indice: usize,
    },
    Solapamiento,
    /// Dos notas nuevas comparten path.
    NotaDuplicada {
        indice: usize,
    },
    /// Path de una nota nueva inválido.
    NotaInvalida {
        indice: usize,
    },
    /// El formato v1 no puede representar notas nuevas.
    NotasNoRepresentables,
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
            ErrorDeltaJson::NotaDuplicada { indice } => {
                write!(f, "nota duplicada en create[{indice}]")
            }
            ErrorDeltaJson::NotaInvalida { indice } => {
                write!(f, "nota inválida en create[{indice}]")
            }
            ErrorDeltaJson::NotasNoRepresentables => {
                write!(f, "el delta v1 no puede representar notas nuevas")
            }
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
        ErrorDelta::NotaDuplicada { indice } => ErrorDeltaJson::NotaDuplicada { indice },
    })
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NotaV2 {
    path: String,
    #[serde(rename = "type")]
    tipo: String,
    title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    body: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DocumentoDeltaV2 {
    schema: String,
    create: Vec<NotaV2>,
    add: Vec<String>,
    remove: Vec<String>,
}

#[derive(Deserialize)]
struct SoloSchema {
    schema: String,
}

/// Parsea `felix-delta-v1` o `felix-delta-v2` según su campo `schema`.
pub fn parsear_delta_json(texto: &str) -> Result<Delta, ErrorDeltaJson> {
    let schema: SoloSchema = serde_json::from_str(texto).map_err(|_| ErrorDeltaJson::Json)?;
    match schema.schema.as_str() {
        SCHEMA_V1 => parsear_delta_json_v1(texto),
        SCHEMA_V2 => parsear_delta_json_v2(texto),
        _ => Err(ErrorDeltaJson::SchemaNoSoportado),
    }
}

/// Parsea el contrato JSON v2: notas nuevas (`create`) además de `add`/`remove`.
/// Aquí sólo se valida la sintaxis del path; tipo, layout y colisiones los
/// valida el núcleo al construir el candidato.
pub fn parsear_delta_json_v2(texto: &str) -> Result<Delta, ErrorDeltaJson> {
    let documento: DocumentoDeltaV2 =
        serde_json::from_str(texto).map_err(|_| ErrorDeltaJson::Json)?;
    if documento.schema != SCHEMA_V2 {
        return Err(ErrorDeltaJson::SchemaNoSoportado);
    }
    let add = parsear_lista(documento.add, LadoDelta::Add)?;
    let remove = parsear_lista(documento.remove, LadoDelta::Remove)?;
    let mut notas = Vec::with_capacity(documento.create.len());
    for (indice, nota) in documento.create.into_iter().enumerate() {
        let path =
            BundlePath::nuevo(&nota.path).map_err(|_| ErrorDeltaJson::NotaInvalida { indice })?;
        if path.como_str() != nota.path {
            return Err(ErrorDeltaJson::NotaInvalida { indice });
        }
        notas.push(NotaNueva {
            path,
            tipo: Sorte::nuevo(&nota.tipo),
            etiqueta: Etiqueta::nuevo(&nota.title),
            cuerpo: nota.body,
        });
    }
    Delta::nuevo(add, remove)
        .and_then(|delta| delta.con_notas(notas))
        .map_err(|error| match error {
            ErrorDelta::Solapamiento => ErrorDeltaJson::Solapamiento,
            ErrorDelta::AtomoNoRepresentable { lado, indice } => {
                ErrorDeltaJson::AtomoNoCanonico { lado, indice }
            }
            ErrorDelta::NotaDuplicada { indice } => ErrorDeltaJson::NotaDuplicada { indice },
        })
}

/// Forma canónica para guardar un delta (contexto de admisión, journal): v1
/// si no crea notas, byte a byte igual que antes de existir v2, y v2 si las
/// crea. Un motor anterior a v2 falla cerrado al leer un v2.
pub fn serializar_delta_json_canonico(delta: &Delta) -> Result<String, ErrorDeltaJson> {
    if delta.crear().is_empty() {
        serializar_delta_json_v1(delta)
    } else {
        serializar_delta_json_v2(delta)
    }
}

/// Serializa en JSON v2 compacto y determinista (notas por path, átomos en
/// orden canónico, `body` omitido si está vacío).
pub fn serializar_delta_json_v2(delta: &Delta) -> Result<String, ErrorDeltaJson> {
    let documento = DocumentoDeltaV2 {
        schema: SCHEMA_V2.to_string(),
        create: delta
            .crear()
            .iter()
            .map(|nota| NotaV2 {
                path: nota.path.como_str().to_string(),
                tipo: nota.tipo.como_str().to_string(),
                title: nota.etiqueta.como_str().to_string(),
                body: nota.cuerpo.clone(),
            })
            .collect(),
        add: delta.agregar().iter().map(ToString::to_string).collect(),
        remove: delta.retirar().iter().map(ToString::to_string).collect(),
    };
    serde_json::to_string(&documento).map_err(|_| ErrorDeltaJson::Json)
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
/// Falla si el delta crea notas: v1 no puede representarlas y descartarlas en
/// silencio rompería el replay de la transacción.
pub fn serializar_delta_json_v1(delta: &Delta) -> Result<String, ErrorDeltaJson> {
    if !delta.crear().is_empty() {
        return Err(ErrorDeltaJson::NotasNoRepresentables);
    }
    let documento = DocumentoDeltaV1 {
        schema: SCHEMA_V1.to_string(),
        add: delta.agregar().iter().map(ToString::to_string).collect(),
        remove: delta.retirar().iter().map(ToString::to_string).collect(),
    };
    serde_json::to_string(&documento).map_err(|_| ErrorDeltaJson::Json)
}
