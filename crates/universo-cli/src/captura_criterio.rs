//! `universo capturar`, `reforzar` y `objetar` (fase 10, U3 y U12): parseo y
//! salida. La lógica vive en `universo_okf::registro_criterio`, compartida con
//! el MCP. **Su salida incluye datos del bundle** (rutas y títulos).
use super::{Formato, escribir_salida, historial::visible};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Write, path::PathBuf};
pub(super) use universo_okf::registro_criterio::Accion;
use universo_okf::{
    registro_criterio::{
        Fallo, MAX_DETALLE, MAX_PROYECTO, MAX_TEXTO, Peticion, Plan, Rutas, VALIDACIONES,
        Veredicto, linea_valida, registrar,
    },
    transaccion::historial::Procedencia,
};

pub(super) const USO: &str = "Uso:\n  universo capturar <texto> --proyecto <nombre> [--tarea <t>] [--validacion explicita|implicita|pendiente] [--fecha AAAA-MM-DD, por defecto la fecha UTC de hoy] [--autor <a>] [--motivo <m>] [--forzar] [--dry-run] [--format text|json]\n  universo reforzar <nota> --proyecto <nombre> --contexto <c> [--validacion ...] [--fecha ...] [--autor <a>] [--motivo <m>] [--dry-run] [--format text|json]\n  universo objetar <nota> --proyecto <nombre> --contexto <c> [--validacion ...] [--fecha ...] [--autor <a>] [--motivo <m>] [--dry-run] [--format text|json]\nDentro de un repositorio de criterio no hacen falta la raíz ni --manifiesto/--invariantes.\n";

pub(super) struct Solicitud {
    accion: Accion,
    raiz: PathBuf,
    manifiesto: PathBuf,
    invariantes: PathBuf,
    /// `capturar`: la afirmación; `reforzar`/`objetar`: la ruta de la nota.
    objetivo: String,
    proyecto: String,
    detalle: Option<String>,
    validacion: String,
    fecha: Option<String>,
    autor: Option<String>,
    motivo: Option<String>,
    forzar: bool,
    dry_run: bool,
    formato: Formato,
}

pub(super) fn parsear(accion: Accion, args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let mut posicionales = Vec::new();
    let mut valores: BTreeMap<&str, String> = BTreeMap::new();
    let (mut forzar, mut dry_run) = (false, false);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--forzar" && accion == Accion::Capturar {
            if std::mem::replace(&mut forzar, true) {
                return Err(fail());
            }
            i += 1;
            continue;
        }
        if arg == "--dry-run" {
            if std::mem::replace(&mut dry_run, true) {
                return Err(fail());
            }
            i += 1;
            continue;
        }
        if !arg.starts_with('-') {
            posicionales.push(arg.clone());
            i += 1;
            continue;
        }
        let (nombre, valor, consumidos) = match arg.split_once('=') {
            Some((n, v)) => (n, v.to_string(), 1),
            None => (arg.as_str(), args.get(i + 1).cloned().ok_or_else(fail)?, 2),
        };
        let clave = match nombre {
            "--manifiesto" => "manifiesto",
            "--invariantes" => "invariantes",
            "--proyecto" => "proyecto",
            "--tarea" if accion == Accion::Capturar => "detalle",
            "--contexto" if accion != Accion::Capturar => "detalle",
            "--validacion" => "validacion",
            "--fecha" => "fecha",
            "--autor" => "autor",
            "--motivo" => "motivo",
            "--format" => "format",
            _ => return Err(fail()),
        };
        if valor.is_empty() || valores.insert(clave, valor).is_some() {
            return Err(fail());
        }
        i += consumidos;
    }
    let (raiz, resto) = posicionales.split_first().ok_or_else(fail)?;
    let objetivo = resto.join(" ");
    let manifiesto = valores.remove("manifiesto").ok_or_else(fail)?;
    let invariantes = valores.remove("invariantes").ok_or_else(fail)?;
    let proyecto = valores.remove("proyecto").ok_or_else(fail)?;
    let detalle = valores.remove("detalle");
    let validacion = valores
        .remove("validacion")
        .unwrap_or_else(|| "pendiente".into());
    let fecha = valores.remove("fecha");
    let formato = match valores.remove("format").as_deref() {
        None | Some("text") => Formato::Texto,
        Some("json") => Formato::Json,
        Some(_) => return Err(fail()),
    };
    let valido = match accion {
        Accion::Capturar => linea_valida(&objetivo, MAX_TEXTO),
        // Reforzar y objetar exigen una nota y el contexto en que se aplicó.
        _ => resto.len() == 1 && detalle.is_some(),
    };
    if !valido
        || !linea_valida(&proyecto, MAX_PROYECTO)
        || !VALIDACIONES.contains(&validacion.as_str())
        || detalle
            .as_deref()
            .is_some_and(|d| !linea_valida(d, MAX_DETALLE))
        || fecha
            .as_deref()
            .is_some_and(|f| !universo_core::manifiesto::es_fecha_iso(f))
    {
        return Err(fail());
    }
    Ok(Solicitud {
        accion,
        raiz: PathBuf::from(raiz),
        manifiesto: PathBuf::from(manifiesto),
        invariantes: PathBuf::from(invariantes),
        objetivo,
        proyecto,
        detalle,
        validacion,
        fecha,
        autor: valores.remove("autor"),
        motivo: valores.remove("motivo"),
        forzar,
        dry_run,
        formato,
    })
}

/// Resultado de un comando, listo para mostrar.
enum Resultado {
    Hecho {
        plan: Plan,
        aplicado: bool,
        sellado: bool,
        /// El historial anotó el cambio; si no, falta su entrada.
        registrado: bool,
    },
    Rechazado {
        reglas: Vec<Value>,
        conflictos: usize,
    },
}

fn correr(s: &Solicitud) -> Result<Resultado, Fallo> {
    let peticion = Peticion {
        accion: s.accion,
        objetivo: s.objetivo.clone(),
        proyecto: s.proyecto.clone(),
        detalle: s.detalle.clone(),
        validacion: s.validacion.clone(),
        fecha: s.fecha.clone(),
        forzar: s.forzar,
    };
    let motivo = s.motivo.clone().unwrap_or_else(|| {
        format!("{}: {}", s.accion.nombre(), s.objetivo.trim())
            .chars()
            .take(500)
            .collect()
    });
    let procedencia = Procedencia {
        autor: s.autor.clone(),
        motivo: Some(motivo),
    };
    let rutas = Rutas {
        raiz: &s.raiz,
        manifiesto: &s.manifiesto,
        invariantes: &s.invariantes,
    };
    let (plan, veredicto) = registrar(&rutas, &peticion, &procedencia, s.dry_run)?;
    Ok(match veredicto {
        Veredicto::Admitido {
            aplicado,
            sellado,
            registrado,
        } => Resultado::Hecho {
            plan,
            aplicado,
            sellado,
            registrado,
        },
        Veredicto::Rechazado { reglas, conflictos } => Resultado::Rechazado { reglas, conflictos },
    })
}

fn texto_hecho(
    comando: &str,
    plan: &Plan,
    aplicado: bool,
    sellado: bool,
    registrado: bool,
) -> String {
    let mut texto = if aplicado {
        format!("{comando}: applied\n")
    } else {
        format!("{comando}: admit (dry run, nothing written)\n")
    };
    texto.push_str(&format!("note: {}\n", plan.nota));
    if let Some(evidencia) = &plan.evidencia {
        texto.push_str(&format!("evidence: {evidencia}\n"));
    }
    let estado = if plan.proyecto_creado {
        "created"
    } else {
        "existing"
    };
    texto.push_str(&format!("project: {} ({estado})\n", plan.proyecto));
    if aplicado {
        let sello = if sellado { "yes" } else { "no" };
        texto.push_str(&format!("sealed: {sello}\n"));
        if !registrado {
            texto.push_str(
                "history: not recorded (el cambio se aplicó, pero falta su entrada en el log)\n",
            );
        }
    }
    texto
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let comando = s.accion.nombre();
    let (payload, codigo) = match (correr(&s), s.formato) {
        (
            Ok(Resultado::Hecho {
                plan,
                aplicado,
                sellado,
                registrado,
            }),
            Formato::Json,
        ) => (
            format!(
                "{}\n",
                json!({"schema_version":1,"command":comando,
                       "status": if aplicado { "applied" } else { "admit" },
                       "dry_run": s.dry_run, "note": plan.nota, "project": plan.proyecto,
                       "project_created": plan.proyecto_creado, "evidence": plan.evidencia,
                       "sealed": sellado, "logged": registrado, "errors":[]})
            ),
            0,
        ),
        (
            Ok(Resultado::Hecho {
                plan,
                aplicado,
                sellado,
                registrado,
            }),
            Formato::Texto,
        ) => (
            visible(&texto_hecho(comando, &plan, aplicado, sellado, registrado)),
            0,
        ),
        (Ok(Resultado::Rechazado { reglas, conflictos }), Formato::Json) => (
            format!(
                "{}\n",
                json!({"schema_version":1,"command":comando,"status":"reject",
                       "rules":reglas,"conflicts":conflictos,"errors":[]})
            ),
            1,
        ),
        (Ok(Resultado::Rechazado { reglas, conflictos }), Formato::Texto) => {
            let mut texto = format!("{comando}: reject\n");
            for regla in &reglas {
                texto.push_str(&format!(
                    "rule: {} — {}\n",
                    regla["rule_id"].as_str().unwrap_or("(sin id)"),
                    regla["rule_message"].as_str().unwrap_or("")
                ));
            }
            if conflictos > 0 {
                texto.push_str(&format!("conflicts: {conflictos}\n"));
            }
            (visible(&texto), 1)
        }
        (Err(f), Formato::Json) => (
            format!(
                "{}\n",
                json!({"schema_version":1,"command":comando,"status":"error",
                       "errors":[{"code":f.codigo,"detail":f.detalle}]})
            ),
            1,
        ),
        (Err(f), Formato::Texto) => {
            let mut texto = format!("{comando}: error\ncode: {}\n", f.codigo);
            if let Some(lista) = f.detalle.as_array() {
                for item in lista {
                    texto.push_str(&format!("- {}\n", item.as_str().unwrap_or_default()));
                }
            }
            if f.codigo == "capture_similar" {
                texto.push_str("hint: usa `universo reforzar <nota>` o repite con --forzar\n");
            }
            (visible(&texto), 1)
        }
    };
    if !escribir_salida(salida, error, payload.as_bytes(), "stdout") {
        return 1;
    }
    codigo
}
