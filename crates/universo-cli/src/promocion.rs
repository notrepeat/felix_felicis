//! `universo promover <nota> --a <universo> [--de <universo>]` (fase 10, U11).
//!
//! Lleva un criterio de un universo al siguiente de la escalera. Arma en el
//! destino el criterio con su texto, validación, fecha original y las
//! relaciones `confirmado_en` a sus proyectos (que crea si faltan), y lo
//! propone allí: **las reglas del destino deciden**. Si se admite, marca la
//! nota de origen como `promovido`. Son dos transacciones, una por universo;
//! si la segunda no llega a aplicarse, repetir el comando la completa sin
//! duplicar el criterio en el destino.
//!
//! Felix no distingue a una persona de un agente en la CLI: que la promoción
//! la confirme el dueño es una convención (ver `AGENTS.md`). **Su salida
//! incluye datos del bundle** (rutas).
use super::{
    Formato,
    captura_criterio::{
        Fallo, Rutas, Veredicto, arista, fallo, proyecto, ruta_libre, simular_y_aplicar, slug,
    },
    criterio::resolver_repo,
    escribir_salida,
    historial::visible,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, io::Write, path::PathBuf};
use universo_core::{Atomo, BundlePath, Estructura};
use universo_okf::{
    busqueda::normalizar,
    transaccion::{
        historial::{Procedencia, fecha_hoy},
        simulacion::VistaSimulacion,
    },
};

pub(super) const USO: &str = "Uso: universo promover <nota> --a <universo> [--de <universo>] [--criterio <dir>] [--autor <a>] [--motivo <m>] [--fecha AAAA-MM-DD] [--dry-run] [--ignorar-version] [--format text|json]\nLleva un criterio al universo --a (desde --de, por defecto el universo por defecto del repositorio). Las reglas del destino deciden.\n";

pub(super) struct Solicitud {
    nota: String,
    de: Option<String>,
    a: String,
    criterio: Option<PathBuf>,
    ignorar_version: bool,
    autor: Option<String>,
    motivo: Option<String>,
    fecha: Option<String>,
    dry_run: bool,
    formato: Formato,
}

pub(super) fn parsear(args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let mut posicionales = Vec::new();
    let mut valores = std::collections::BTreeMap::new();
    let (mut dry_run, mut ignorar_version) = (false, false);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let bandera = match arg.as_str() {
            "--dry-run" => Some(&mut dry_run),
            "--ignorar-version" => Some(&mut ignorar_version),
            _ => None,
        };
        if let Some(bandera) = bandera {
            if std::mem::replace(bandera, true) {
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
            "--a" => "a",
            "--de" => "de",
            "--criterio" => "criterio",
            "--autor" => "autor",
            "--motivo" => "motivo",
            "--fecha" => "fecha",
            "--format" => "format",
            _ => return Err(fail()),
        };
        if valor.is_empty() || valor.starts_with('-') || valores.insert(clave, valor).is_some() {
            return Err(fail());
        }
        i += consumidos;
    }
    let [nota] = posicionales.as_slice() else {
        return Err(fail());
    };
    let formato = match valores.remove("format").as_deref() {
        None | Some("text") => Formato::Texto,
        Some("json") => Formato::Json,
        Some(_) => return Err(fail()),
    };
    let fecha = valores.remove("fecha");
    if fecha
        .as_deref()
        .is_some_and(|f| !universo_core::manifiesto::es_fecha_iso(f))
    {
        return Err(fail());
    }
    Ok(Solicitud {
        nota: nota.clone(),
        de: valores.remove("de"),
        a: valores.remove("a").ok_or_else(fail)?,
        criterio: valores.remove("criterio").map(PathBuf::from),
        ignorar_version,
        autor: valores.remove("autor"),
        motivo: valores.remove("motivo"),
        fecha,
        dry_run,
        formato,
    })
}

/// Valor del atributo `predicado` de `nota`, si tiene uno.
fn atributo(e: &Estructura, nota: &str, predicado: &str) -> Option<String> {
    e.atomos().into_iter().find_map(|a| match a {
        Atomo::Ext {
            predicado: p,
            sujeto,
            constante,
        } if p == predicado && sujeto.como_str() == nota => Some(constante),
        _ => None,
    })
}

/// El criterio del destino que viene de esta nota (lleva su marca de
/// procedencia), si una promoción anterior llegó hasta allí. Otro criterio con
/// el mismo título pero sin esa marca es un conflicto, no una promoción a
/// medias.
fn en_destino(e: &Estructura, marca: &str, titulo: &str) -> Result<Option<String>, Fallo> {
    let criterios = || e.nodos().filter(|(_, n)| n.tipo.como_str() == "criterio");
    if let Some((_, n)) =
        criterios().find(|(id, _)| e.anexo(*id).is_some_and(|a| a.cuerpo.contains(marca)))
    {
        return Ok(Some(n.path.to_string()));
    }
    let buscado = normalizar(titulo.trim());
    let iguales: Vec<String> = criterios()
        .filter(|(_, n)| normalizar(n.etiqueta.como_str().trim()) == buscado)
        .map(|(_, n)| n.path.to_string())
        .collect();
    if iguales.is_empty() {
        Ok(None)
    } else {
        Err(Fallo {
            codigo: "promote_conflict",
            detalle: json!(iguales),
        })
    }
}

struct Resumen {
    /// Universos donde el cambio se aplicó pero no quedó en el historial, o
    /// no se pudo sellar.
    sin_historial: Vec<String>,
    sin_sello: Vec<String>,
    origen: String,
    destino: String,
    destino_existia: bool,
    proyectos: Vec<(String, bool)>,
    aplicado: bool,
}

enum Resultado {
    Hecho(Resumen),
    Rechazado {
        universo: String,
        reglas: Vec<Value>,
        conflictos: usize,
    },
}

fn de_criterio(e: universo_okf::criterio::ErrorCriterio) -> Fallo {
    Fallo {
        codigo: e.codigo(),
        detalle: json!([e.to_string()]),
    }
}

fn correr(s: &Solicitud) -> Result<Resultado, Fallo> {
    let repo = resolver_repo(s.criterio.clone(), s.ignorar_version).map_err(de_criterio)?;
    let origen = repo.universo(s.de.as_deref()).map_err(de_criterio)?;
    let destino = repo.universo(Some(&s.a)).map_err(de_criterio)?;
    if origen.nombre == destino.nombre {
        return Err(fallo("promote_same_universe"));
    }
    let fecha = s.fecha.clone().unwrap_or_else(fecha_hoy);
    let procedencia = Procedencia {
        autor: s.autor.clone(),
        motivo: Some(s.motivo.clone().unwrap_or_else(|| {
            format!(
                "promover {} de {} a {}",
                s.nota, origen.nombre, destino.nombre
            )
            .chars()
            .take(500)
            .collect()
        })),
    };

    // Origen: la nota, su estado y lo que se copia.
    let (bundle_o, manifiesto_o, invariantes_o) =
        (origen.bundle(), origen.manifiesto(), origen.invariantes());
    let vista_o = VistaSimulacion::capturar(&bundle_o, &manifiesto_o, &invariantes_o)
        .map_err(|e| fallo(e.codigo()))?;
    let eo = vista_o.estructura();
    let ruta = BundlePath::nuevo(&s.nota).map_err(|_| fallo("note_not_found"))?;
    let id = eo
        .por_path(&ruta)
        .filter(|id| {
            eo.nodo(*id)
                .is_some_and(|n| n.tipo.como_str() == "criterio")
        })
        .ok_or_else(|| fallo("note_not_found"))?;
    let nodo = eo.nodo(id).ok_or_else(|| fallo("note_not_found"))?;
    let nota = ruta.to_string();
    let titulo = nodo.etiqueta.como_str().to_string();
    let cuerpo = eo.anexo(id).map(|a| a.cuerpo.clone()).unwrap_or_default();
    match atributo(eo, &nota, "Estado").as_deref() {
        Some("promovido") => return Err(fallo("promote_already_done")),
        Some("candidato") | None => {}
        Some(_) => return Err(fallo("promote_state_invalid")),
    }
    let validacion = atributo(eo, &nota, "Validacion");
    let fecha_original = atributo(eo, &nota, "Fecha");
    let titulos_proyectos: Vec<String> = eo
        .atomos()
        .into_iter()
        .filter_map(|a| match a {
            Atomo::Edge {
                origen,
                destino,
                sorte,
                ..
            } if origen.como_str() == nota && sorte.como_str() == "confirmado_en" => {
                let id = eo.por_path(&destino)?;
                Some(eo.nodo(id)?.etiqueta.como_str().to_string())
            }
            _ => None,
        })
        .collect();
    // Dos proyectos con el mismo nombre normalizado cuentan una vez.
    let mut vistos = BTreeSet::new();
    let titulos_proyectos: Vec<String> = titulos_proyectos
        .into_iter()
        .filter(|t| vistos.insert(slug(t)))
        .collect();
    let marca = format!("Promovido desde {} ({nota})", origen.nombre);

    // Destino: crear el criterio, salvo que ya esté (promoción a medias).
    let (bundle_d, manifiesto_d, invariantes_d) = (
        destino.bundle(),
        destino.manifiesto(),
        destino.invariantes(),
    );
    let vista_d = VistaSimulacion::capturar(&bundle_d, &manifiesto_d, &invariantes_d)
        .map_err(|e| fallo(e.codigo()))?;
    let ed = vista_d.estructura();
    let mut proyectos = Vec::new();
    let mut sin_historial = Vec::new();
    let mut sin_sello = Vec::new();
    let (en_destino, destino_existia) = match en_destino(ed, &marca, &titulo)? {
        Some(existente) => (existente, true),
        None => {
            let mut reservados = BTreeSet::new();
            let base = nota.rsplit('/').next().unwrap_or_default().to_string();
            let nueva = ruta_libre(ed, "criterios", &base, &reservados);
            reservados.insert(nueva.rsplit('/').next().unwrap_or_default().to_lowercase());
            let mut crear = vec![json!({"path": nueva, "type": "criterio", "title": titulo,
                "body": format!("{}\n\n{marca} el {fecha}.\n", cuerpo.trim_end())})];
            let mut agregar = vec![format!("Estado({nueva}, vigente)")];
            if let Some(v) = &validacion {
                agregar.push(format!("Validacion({nueva}, {v})"));
            }
            if let Some(f) = &fecha_original {
                agregar.push(format!("Fecha({nueva}, {f})"));
            }
            for titulo_p in &titulos_proyectos {
                let (ruta_p, creado) = proyecto(ed, titulo_p, &reservados);
                if creado {
                    reservados.insert(ruta_p.rsplit('/').next().unwrap_or_default().to_lowercase());
                    crear.push(json!({"path": ruta_p, "type": "proyecto", "title": titulo_p}));
                }
                agregar.extend(arista(ed, &nueva, &ruta_p, "confirmado_en"));
                proyectos.push((ruta_p, creado));
            }
            let delta = json!({"schema":"felix-delta-v2","create":crear,"update":[],"add":agregar,"remove":[]});
            let rutas = Rutas {
                raiz: &bundle_d,
                manifiesto: &manifiesto_d,
                invariantes: &invariantes_d,
            };
            match simular_y_aplicar(&vista_d, &rutas, &delta, &procedencia, s.dry_run)? {
                Veredicto::Rechazado { reglas, conflictos } => {
                    return Ok(Resultado::Rechazado {
                        universo: destino.nombre.clone(),
                        reglas,
                        conflictos,
                    });
                }
                Veredicto::Admitido {
                    aplicado,
                    sellado,
                    registrado,
                } => {
                    if aplicado && !registrado {
                        sin_historial.push(destino.nombre.clone());
                    }
                    if aplicado && !sellado {
                        sin_sello.push(destino.nombre.clone());
                    }
                }
            }
            (nueva, false)
        }
    };
    // Origen: marcar la candidata como promovida. En --dry-run también se
    // simula, para que un rechazo del origen se vea antes de escribir nada.
    let delta = json!({"schema":"felix-delta-v2","create":[],
        "update":[{"path": nota, "body": format!("{}\n\nPromovido a {} el {fecha} ({en_destino}).\n",
                                                  cuerpo.trim_end(), destino.nombre)}],
        "add":[format!("Estado({nota}, promovido)")],
        "remove": atributo(eo, &nota, "Estado").map(|e| vec![format!("Estado({nota}, {e})")]).unwrap_or_default()});
    let rutas = Rutas {
        raiz: &bundle_o,
        manifiesto: &manifiesto_o,
        invariantes: &invariantes_o,
    };
    match simular_y_aplicar(&vista_o, &rutas, &delta, &procedencia, s.dry_run)? {
        Veredicto::Admitido {
            aplicado,
            sellado,
            registrado,
        } => {
            if aplicado && !registrado {
                sin_historial.push(origen.nombre.clone());
            }
            if aplicado && !sellado {
                sin_sello.push(origen.nombre.clone());
            }
            Ok(Resultado::Hecho(Resumen {
                sin_historial,
                sin_sello,
                origen: format!("{}/{nota}", origen.nombre),
                destino: format!("{}/{en_destino}", destino.nombre),
                destino_existia,
                proyectos,
                aplicado,
            }))
        }
        Veredicto::Rechazado { reglas, conflictos } if s.dry_run || destino_existia => {
            Ok(Resultado::Rechazado {
                universo: origen.nombre.clone(),
                reglas,
                conflictos,
            })
        }
        // El destino ya se escribió: decirlo, con la regla que falla.
        Veredicto::Rechazado { reglas, .. } => {
            let mut detalle: Vec<String> = reglas
                .iter()
                .map(|r| {
                    format!(
                        "rule: {} — {}",
                        r["rule_id"].as_str().unwrap_or("(sin id)"),
                        r["rule_message"].as_str().unwrap_or("")
                    )
                })
                .collect();
            detalle.push(format!(
                "el criterio ya está en {}; corrige el origen y repite `universo promover` para marcarlo",
                destino.nombre
            ));
            Err(Fallo {
                codigo: "promote_source_rejected",
                detalle: json!(detalle),
            })
        }
    }
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let (payload, codigo) = match (correr(&s), s.formato) {
        (Ok(Resultado::Hecho(r)), Formato::Json) => (
            format!(
                "{}\n",
                json!({"schema_version":1,"command":"promover",
                       "status": if r.aplicado { "applied" } else { "admit" },
                       "dry_run": s.dry_run, "from": r.origen, "to": r.destino,
                       "to_existed": r.destino_existia,
                       "not_logged": r.sin_historial, "not_sealed": r.sin_sello,
                       "projects": r.proyectos.iter().map(|(p, c)| json!({"path": p, "created": c})).collect::<Vec<_>>(),
                       "errors": []})
            ),
            0,
        ),
        (Ok(Resultado::Hecho(r)), Formato::Texto) => {
            let mut texto = if r.aplicado {
                "promover: applied\n".to_string()
            } else {
                "promover: admit (dry run, nothing written)\n".to_string()
            };
            let marca = if r.aplicado { " (now promovido)" } else { "" };
            texto.push_str(&format!("from: {}{marca}\n", r.origen));
            let existia = if r.destino_existia { " (existing)" } else { "" };
            texto.push_str(&format!("to: {}{existia}\n", r.destino));
            for (p, creado) in &r.proyectos {
                let estado = if *creado { "created" } else { "existing" };
                texto.push_str(&format!("project: {p} ({estado})\n"));
            }
            for u in &r.sin_historial {
                texto.push_str(&format!("history: not recorded in {u}\n"));
            }
            for u in &r.sin_sello {
                texto.push_str(&format!("sealed: no in {u}\n"));
            }
            (visible(&texto), 0)
        }
        (
            Ok(Resultado::Rechazado {
                universo,
                reglas,
                conflictos,
            }),
            Formato::Json,
        ) => (
            format!(
                "{}\n",
                json!({"schema_version":1,"command":"promover","status":"reject","universe":universo,
                       "rules":reglas,"conflicts":conflictos,"errors":[]})
            ),
            1,
        ),
        (
            Ok(Resultado::Rechazado {
                universo,
                reglas,
                conflictos,
            }),
            Formato::Texto,
        ) => {
            let mut texto = format!("promover: reject\nuniverse: {universo}\n");
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
                json!({"schema_version":1,"command":"promover","status":"error",
                       "errors":[{"code":f.codigo,"detail":f.detalle}]})
            ),
            1,
        ),
        (Err(f), Formato::Texto) => {
            let mut texto = format!("promover: error\ncode: {}\n", f.codigo);
            if let Some(lista) = f.detalle.as_array() {
                for item in lista {
                    texto.push_str(&format!("- {}\n", item.as_str().unwrap_or_default()));
                }
            }
            (visible(&texto), 1)
        }
    };
    if !escribir_salida(salida, error, payload.as_bytes(), "stdout") {
        return 1;
    }
    codigo
}
