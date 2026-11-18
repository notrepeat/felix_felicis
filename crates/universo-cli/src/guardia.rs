//! `guard` y `seal`: detección redactada de cambios hechos fuera de Felix.
use super::{Formato, escribir_salida};
use serde_json::json;
use std::{io::Write, path::PathBuf};
use universo_okf::transaccion::sello::{EstadoSello, evaluar_guardia, sellar};

pub(super) const USO: &str = "Uso:\n  universo guard <raiz> --manifiesto <m.toml> --invariantes <i.sx> [--format text|json]\n  universo seal <raiz> --manifiesto <m.toml> --invariantes <i.sx> [--format text|json]\n";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Accion {
    Guard,
    Seal,
}

pub(super) struct Solicitud {
    accion: Accion,
    raiz: PathBuf,
    manifiesto: PathBuf,
    invariantes: PathBuf,
    formato: Formato,
}

pub(super) fn parsear(accion: Accion, args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let (mut raiz, mut manifiesto, mut invariantes, mut formato) = (None, None, None, None);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if !arg.starts_with('-') {
            if arg.is_empty() || raiz.replace(PathBuf::from(arg)).is_some() {
                return Err(fail());
            }
            i += 1;
            continue;
        }
        let (nombre, valor, consumidos) = if let Some((nombre, valor)) = arg.split_once('=') {
            (nombre, valor, 1)
        } else {
            let valor = args.get(i + 1).ok_or_else(fail)?;
            if valor.starts_with('-') {
                return Err(fail());
            }
            (arg.as_str(), valor.as_str(), 2)
        };
        if valor.is_empty() {
            return Err(fail());
        }
        match nombre {
            "--manifiesto" if manifiesto.is_none() => manifiesto = Some(PathBuf::from(valor)),
            "--invariantes" if invariantes.is_none() => invariantes = Some(PathBuf::from(valor)),
            "--format" if formato.is_none() => {
                formato = Some(match valor {
                    "text" => Formato::Texto,
                    "json" => Formato::Json,
                    _ => return Err(fail()),
                })
            }
            _ => return Err(fail()),
        }
        i += consumidos;
    }
    Ok(Solicitud {
        accion,
        raiz: raiz.ok_or_else(fail)?,
        manifiesto: manifiesto.ok_or_else(fail)?,
        invariantes: invariantes.ok_or_else(fail)?,
        formato: formato.unwrap_or(Formato::Texto),
    })
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let (payload, codigo) = match s.accion {
        Accion::Guard => guard(&s),
        Accion::Seal => seal(&s),
    };
    if !escribir_salida(salida, error, payload.as_bytes(), "stdout") {
        return 1;
    }
    codigo
}

fn guard(s: &Solicitud) -> (String, i32) {
    let guardia = match evaluar_guardia(&s.raiz, &s.manifiesto, &s.invariantes) {
        Ok(guardia) => guardia,
        Err(e) => return (error_payload("guard", e.codigo(), s.formato), 1),
    };
    let status = if guardia.aprobada() { "pass" } else { "fail" };
    let transaction = if guardia.transaccion_limpia {
        "clean"
    } else {
        "pending"
    };
    let invariants = if guardia.invariantes_fallidas == 0 {
        "admit"
    } else {
        "reject"
    };
    let seal = match guardia.sello {
        EstadoSello::Coincide => "match",
        EstadoSello::Difiere => "mismatch",
        EstadoSello::Ausente => "absent",
    };
    let payload = match s.formato {
        Formato::Json => format!(
            "{}\n",
            json!({"schema_version":1,"command":"guard","status":status,"transaction":transaction,
                   "invariants":invariants,"failed_invariants":guardia.invariantes_fallidas,"seal":seal})
        ),
        Formato::Texto => {
            let mut text = format!(
                "guard: {status}\ntransaction: {transaction}\ninvariants: {invariants}; failed: {}\nseal: {seal}\n",
                guardia.invariantes_fallidas
            );
            match guardia.sello {
                EstadoSello::Difiere => text.push_str("note: el bundle, el manifiesto o las invariantes cambiaron fuera de Felix desde el último estado admitido. Revisa el cambio; si es correcto, ejecuta `universo seal`.\n"),
                EstadoSello::Ausente => text.push_str("note: no hay sello del último estado admitido. Si el estado actual es correcto, ejecuta `universo seal`.\n"),
                EstadoSello::Coincide => {}
            }
            if !guardia.transaccion_limpia {
                text.push_str("note: hay una transacción pendiente; ejecuta `universo status`.\n");
            }
            text
        }
    };
    (payload, if guardia.aprobada() { 0 } else { 1 })
}

fn seal(s: &Solicitud) -> (String, i32) {
    match sellar(&s.raiz, &s.manifiesto, &s.invariantes) {
        Ok(()) => (
            match s.formato {
                Formato::Json => format!(
                    "{}\n",
                    json!({"schema_version":1,"command":"seal","status":"ok","errors":[]})
                ),
                Formato::Texto => "seal: ok\n".to_string(),
            },
            0,
        ),
        Err(e) => (error_payload("seal", e.codigo(), s.formato), 1),
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
