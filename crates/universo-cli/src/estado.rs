//! `status` y `recover`: estado transaccional redactado (sin nombres ni rutas).
use super::{Formato, escribir_salida};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use universo_okf::transaccion::estado::{EstadoTransaccional, inspeccionar, recuperar};

pub(super) const USO: &str = "Uso:\n  universo status <raiz> [--format text|json]\n  universo recover <raiz> [--format text|json]\n";

const NOTA_STAGE_HUERFANO: &str = "orphan stage: hay un stage sin journal junto al root. Felix no puede probar que es propio, así que no lo borra y todo apply fallará con apply_recovery_unverified. Si ningún otro proceso Felix está escribiendo, revisa los directorios .<raiz>.felix.stage.* junto al root y bórralos a mano.\n";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Accion {
    Status,
    Recover,
}

pub(super) struct Solicitud {
    accion: Accion,
    raiz: PathBuf,
    formato: Formato,
}

pub(super) fn parsear(accion: Accion, args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let mut raiz = None;
    let mut formato = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let valor_format = if let Some(valor) = arg.strip_prefix("--format=") {
            i += 1;
            Some(valor)
        } else if arg == "--format" {
            let valor = args.get(i + 1).ok_or_else(fail)?;
            i += 2;
            Some(valor.as_str())
        } else {
            None
        };
        if let Some(valor) = valor_format {
            let nuevo = match valor {
                "text" => Formato::Texto,
                "json" => Formato::Json,
                _ => return Err(fail()),
            };
            if formato.replace(nuevo).is_some() {
                return Err(fail());
            }
            continue;
        }
        if arg.is_empty() || arg.starts_with('-') || raiz.replace(PathBuf::from(arg)).is_some() {
            return Err(fail());
        }
        i += 1;
    }
    Ok(Solicitud {
        accion,
        raiz: raiz.ok_or_else(fail)?,
        formato: formato.unwrap_or(Formato::Texto),
    })
}

pub(super) fn ejecutar(
    solicitud: Solicitud,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    let (payload, codigo) = match solicitud.accion {
        Accion::Status => status(&solicitud.raiz, solicitud.formato),
        Accion::Recover => recover(&solicitud.raiz, solicitud.formato),
    };
    if !escribir_salida(salida, error, payload.as_bytes(), "stdout") {
        return 1;
    }
    codigo
}

/// Aviso para `check`: `None` si no hay estado pendiente o no se pudo leer.
pub(super) fn aviso_pendiente(raiz: &Path) -> Option<&'static str> {
    let estado = inspeccionar(raiz).ok()?;
    if estado.limpio() || estado.sin_rastro() {
        None
    } else if estado.stage_sin_journal() {
        Some(
            "warning: transacción pendiente (stage sin journal); el resultado refleja el root actual. Ejecuta `universo status <raiz>`.\n",
        )
    } else {
        Some(
            "warning: transacción pendiente o interrumpida; el resultado refleja el root actual. Ejecuta `universo status <raiz>` y `universo recover <raiz>`.\n",
        )
    }
}

fn presencia(valor: bool) -> &'static str {
    if valor { "present" } else { "absent" }
}

fn resumen(estado: &EstadoTransaccional) -> Value {
    json!({
        "root_present": estado.raiz_presente,
        "journal": estado.journal,
        "context": estado.contexto,
        "retire_intent": estado.intent_retiro,
        "backup": estado.backup,
        "stages": estado.stages.len(),
        "other_reserved": estado.otros.len(),
        "orphan_stage": estado.stage_sin_journal(),
    })
}

fn status(raiz: &Path, formato: Formato) -> (String, i32) {
    let estado = match inspeccionar(raiz) {
        Ok(estado) => estado,
        Err(e) => return (error_payload("status", e.codigo(), formato), 1),
    };
    let status = if estado.limpio() {
        "clean"
    } else if estado.sin_rastro() {
        "missing"
    } else {
        "pending"
    };
    let codigo = if estado.limpio() { 0 } else { 1 };
    let payload = match formato {
        Formato::Json => {
            let mut value = resumen(&estado);
            value["schema_version"] = json!(1);
            value["command"] = json!("status");
            value["status"] = json!(status);
            format!("{value}\n")
        }
        Formato::Texto => {
            let mut text = format!(
                "status: {status}\nroot: {}\njournal: {}\ncontext: {}\nretire intent: {}\nbackup: {}\nstages: {}\nother reserved: {}\n",
                presencia(estado.raiz_presente),
                presencia(estado.journal),
                presencia(estado.contexto),
                presencia(estado.intent_retiro),
                presencia(estado.backup),
                estado.stages.len(),
                estado.otros.len(),
            );
            if estado.sin_rastro() {
                text.push_str("note: el root no existe y no hay ninguna transacción pendiente; revisa la ruta.\n");
            } else if estado.stage_sin_journal() {
                text.push_str(NOTA_STAGE_HUERFANO);
            } else if !estado.limpio() {
                text.push_str("note: ejecuta `universo recover <raiz>` para completar o revertir la transacción interrumpida.\n");
            }
            text
        }
    };
    (payload, codigo)
}

fn recover(raiz: &Path, formato: Formato) -> (String, i32) {
    match recuperar(raiz) {
        Ok(()) => (
            match formato {
                Formato::Json => format!(
                    "{}\n",
                    json!({"schema_version":1,"command":"recover","status":"ok","errors":[]})
                ),
                Formato::Texto => "recover: ok\n".to_string(),
            },
            0,
        ),
        Err(e) => (error_payload("recover", e.codigo(), formato), 1),
    }
}

fn error_payload(command: &str, code: &str, formato: Formato) -> String {
    match formato {
        Formato::Json => format!(
            "{}\n",
            json!({"schema_version":1,"command":command,"status":"error","errors":[{"code":code}]})
        ),
        Formato::Texto => format!("{command}: error\ncode: {code}\n"),
    }
}
