//! Salida redactada para el modo apply de propose.
use super::{Formato, escribir_salida};
use serde_json::{Value, json};
use std::{io::Write, path::PathBuf};
use universo_okf::transaccion::{
    aplicar::{EstadoAplicacion, aplicar_archivo},
    snapshot::SnapshotToken,
};

pub(super) struct SolicitudApply {
    pub(super) raiz: PathBuf,
    pub(super) manifiesto: PathBuf,
    pub(super) invariantes: PathBuf,
    pub(super) delta: PathBuf,
    pub(super) formato: Formato,
    pub(super) esperado: String,
}
pub(super) fn ejecutar(
    solicitud: SolicitudApply,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    let result = SnapshotToken::from_hex(&solicitud.esperado)
        .map_err(|e| e.codigo())
        .and_then(|token| {
            aplicar_archivo(
                &solicitud.raiz,
                &solicitud.manifiesto,
                &solicitud.invariantes,
                &solicitud.delta,
                token,
            )
            .map_err(|e| e.codigo())
        });
    let (status, summary, errors) = match result {
        Ok(result) => (
            if result.estado == EstadoAplicacion::Aplicada {
                "admit"
            } else {
                "reject"
            },
            json!({"conflicts":result.conflictos,"failed_invariants":result.invariantes_fallidas}),
            json!([]),
        ),
        Err(code) => ("error", Value::Null, json!([{"code":code}])),
    };
    let payload = match solicitud.formato {
        Formato::Json => format!(
            "{}\n",
            json!({"schema_version":1,"command":"propose","mode":"apply","status":status,"summary":summary,"errors":errors})
        ),
        Formato::Texto => {
            let mut text = format!("propose apply: {status}\n");
            if status == "error" {
                text.push_str(&format!("code: {}\n", errors[0]["code"].as_str().unwrap()));
            } else {
                text.push_str(&format!(
                    "conflicts: {}; failed_invariants: {}\n",
                    summary["conflicts"], summary["failed_invariants"]
                ));
            }
            text
        }
    };
    if !escribir_salida(salida, error, payload.as_bytes(), "stdout") || status != "admit" {
        1
    } else {
        0
    }
}
