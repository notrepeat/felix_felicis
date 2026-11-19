//! `universo resumen` (fase 10, U8): cómo está cada universo del repositorio
//! de criterio. Conteos, rutas y los últimos cambios, sin cuerpos: sirve para
//! decidir qué revisar o promover. **Su salida incluye datos del bundle**
//! (rutas).
use super::{Formato, criterio::resolver_repo, escribir_salida, historial::visible};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::PathBuf,
};
use universo_core::{Atomo, Estructura};
use universo_okf::transaccion::{
    estado::inspeccionar, historial::leer, simulacion::VistaSimulacion,
};

pub(super) const USO: &str = "Uso: universo resumen [--criterio <dir>] [--ignorar-version] [--format text|json]\nPor universo: notas por tipo, criterios por estado, relaciones, reglas, candidatas listas para promover, objetadas (candidatas o vigentes), sin relaciones y últimos cambios.\n";

pub(super) struct Solicitud {
    criterio: Option<PathBuf>,
    ignorar_version: bool,
    formato: Formato,
}

pub(super) fn parsear(args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let (mut criterio, mut formato, mut ignorar_version) = (None, None, false);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--ignorar-version" {
            if std::mem::replace(&mut ignorar_version, true) {
                return Err(fail());
            }
            i += 1;
            continue;
        }
        let (nombre, valor, consumidos) = match arg.split_once('=') {
            Some((n, v)) => (n, v.to_string(), 1),
            None => (arg.as_str(), args.get(i + 1).cloned().ok_or_else(fail)?, 2),
        };
        if valor.is_empty() || valor.starts_with('-') {
            return Err(fail());
        }
        let repetido = match nombre {
            "--criterio" => criterio.replace(PathBuf::from(valor)).is_some(),
            "--format" => {
                let f = match valor.as_str() {
                    "text" => Formato::Texto,
                    "json" => Formato::Json,
                    _ => return Err(fail()),
                };
                formato.replace(f).is_some()
            }
            _ => return Err(fail()),
        };
        if repetido {
            return Err(fail());
        }
        i += consumidos;
    }
    Ok(Solicitud {
        criterio,
        ignorar_version,
        formato: formato.unwrap_or(Formato::Texto),
    })
}

/// Resumen de un universo, ya en JSON.
pub(super) fn resumir(
    nombre: &str,
    e: &Estructura,
    reglas: usize,
    raiz: &std::path::Path,
    pendiente: bool,
) -> Value {
    let atomos = e.atomos();
    let mut notas: BTreeMap<String, usize> = BTreeMap::new();
    for (_, n) in e.nodos() {
        *notas.entry(n.tipo.como_str().to_string()).or_default() += 1;
    }
    // Una sola pasada: tipo de cada nota por ruta.
    let tipos: BTreeMap<String, String> = e
        .nodos()
        .map(|(_, n)| (n.path.to_string(), n.tipo.como_str().to_string()))
        .collect();
    let tipo = |path: &str| tipos.get(path).cloned();
    let mut estados: BTreeMap<String, usize> = BTreeMap::new();
    let mut estado_de: BTreeMap<String, String> = BTreeMap::new();
    let mut proyectos: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut objetadas = BTreeSet::new();
    let mut con_relacion = BTreeSet::new();
    let mut relaciones = 0;
    for a in &atomos {
        match a {
            Atomo::Ext {
                predicado,
                sujeto,
                constante,
            } if predicado == "Estado"
                && tipo(sujeto.como_str()).as_deref() == Some("criterio") =>
            {
                *estados.entry(constante.clone()).or_default() += 1;
                estado_de.insert(sujeto.to_string(), constante.clone());
            }
            Atomo::Edge {
                origen,
                destino,
                sorte,
                ..
            } => {
                relaciones += 1;
                con_relacion.insert(origen.to_string());
                con_relacion.insert(destino.to_string());
                match sorte.como_str() {
                    "confirmado_en" if tipo(destino.como_str()).as_deref() == Some("proyecto") => {
                        proyectos
                            .entry(origen.to_string())
                            .or_default()
                            .insert(destino.to_string());
                    }
                    "objeta" => {
                        objetadas.insert(destino.to_string());
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    // Cuentan las objeciones a candidatas y a vigentes (una objeción no retira
    // un vigente: se marca para revisarlo). Lo promovido o retirado ya se
    // decidió.
    let objetadas: BTreeSet<String> = objetadas
        .into_iter()
        .filter(|n| {
            matches!(
                estado_de.get(n).map(String::as_str),
                Some("candidato" | "vigente")
            )
        })
        .collect();
    // Lo mismo que exige la regla `vigente-confirmado` de la plantilla: una
    // candidata confirmada en al menos dos proyectos distintos. Está escrito
    // aquí, no leído de las reglas de vigente: si se cambian, puede divergir
    // de lo que `promover --dry-run` admita.
    let listas: Vec<Value> = proyectos
        .iter()
        .filter(|(nota, p)| {
            p.len() >= 2 && estado_de.get(*nota).map(String::as_str) == Some("candidato")
        })
        .map(|(nota, p)| json!({"path": nota, "projects": p.len()}))
        .collect();
    let sin_relaciones: Vec<String> = e
        .nodos()
        .filter(|(_, n)| {
            n.tipo.como_str() == "criterio" && !con_relacion.contains(n.path.como_str())
        })
        .map(|(_, n)| n.path.to_string())
        .collect();
    let recientes: Vec<Value> = leer(raiz)
        .map(|h| {
            h.entradas
                .iter()
                .rev()
                .take(3)
                .map(|e| json!({"time": e.fecha, "author": e.autor, "reason": e.motivo}))
                .collect()
        })
        .unwrap_or_default();
    json!({"name": nombre, "status": if pendiente { "pending_transaction" } else { "ok" },
           "notes": notas, "criterios_by_state": estados,
           "relations": relaciones, "rules": reglas, "ready_to_promote": listas,
           "objected": objetadas, "without_relations": sin_relaciones, "recent": recientes})
}

fn texto(universos: &[Value]) -> String {
    let lista = |v: &Value| -> String {
        v.as_array()
            .map(|a| {
                a.iter()
                    .map(|x| {
                        x.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| x.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    let conteos = |v: &Value| -> String {
        v.as_object()
            .map(|o| {
                o.iter()
                    .map(|(k, n)| format!("{k} {n}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    let mut salida = String::new();
    for u in universos {
        salida.push_str(&format!("universe: {}\n", u["name"].as_str().unwrap_or("")));
        match u["status"].as_str() {
            Some("ok") | None => {}
            Some("pending_transaction") => salida.push_str(
                "  aviso: hay una transacción pendiente; `universo status` y `universo recover`\n",
            ),
            Some(otro) => {
                salida.push_str(&format!("  invalid: {otro}\n"));
                continue;
            }
        }
        salida.push_str(&format!("  notes: {}\n", conteos(&u["notes"])));
        salida.push_str(&format!(
            "  criterios by state: {}\n",
            conteos(&u["criterios_by_state"])
        ));
        salida.push_str(&format!(
            "  relations: {}; rules: {}\n",
            u["relations"], u["rules"]
        ));
        for r in u["ready_to_promote"].as_array().into_iter().flatten() {
            salida.push_str(&format!(
                "  ready to promote: {} ({} projects)\n",
                r["path"].as_str().unwrap_or(""),
                r["projects"]
            ));
        }
        if u["objected"].as_array().is_some_and(|a| !a.is_empty()) {
            salida.push_str(&format!("  objected: {}\n", lista(&u["objected"])));
        }
        if u["without_relations"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
        {
            salida.push_str(&format!(
                "  without relations: {}\n",
                lista(&u["without_relations"])
            ));
        }
        for r in u["recent"].as_array().into_iter().flatten() {
            salida.push_str(&format!(
                "  recent: {} author={} reason={}\n",
                r["time"].as_str().unwrap_or(""),
                r["author"].as_str().unwrap_or("none"),
                r["reason"].as_str().unwrap_or("none")
            ));
        }
    }
    visible(&salida)
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let repo = match resolver_repo(s.criterio, s.ignorar_version) {
        Ok(repo) => repo,
        Err(e) => {
            let texto = match s.formato {
                Formato::Json => format!(
                    "{}\n",
                    json!({"schema_version":1,"command":"resumen","status":"error",
                           "errors":[{"code":e.codigo(),"message":e.to_string()}]})
                ),
                Formato::Texto => format!("resumen: error\ncode: {}\n{e}\n", e.codigo()),
            };
            escribir_salida(salida, error, texto.as_bytes(), "stdout");
            return 1;
        }
    };
    let mut universos = Vec::new();
    for nombre in repo.universos.keys() {
        // Un universo que no carga se marca y no impide ver los demás.
        let invalido = |codigo: &str| json!({"name": nombre, "status": codigo});
        let u = match repo.universo(Some(nombre)) {
            Ok(u) => u,
            Err(e) => {
                universos.push(invalido(e.codigo()));
                continue;
            }
        };
        let pendiente = inspeccionar(&u.bundle()).map_or(true, |estado| !estado.limpio());
        match VistaSimulacion::capturar(&u.bundle(), &u.manifiesto(), &u.invariantes()) {
            Ok(vista) => universos.push(resumir(
                nombre,
                vista.estructura(),
                vista.invariantes().len(),
                &u.bundle(),
                pendiente,
            )),
            Err(e) => universos.push(invalido(e.codigo())),
        }
    }
    let bytes = match s.formato {
        Formato::Texto => texto(&universos),
        Formato::Json => format!(
            "{}\n",
            json!({"schema_version":1,"command":"resumen","universes":universos})
        ),
    };
    if escribir_salida(salida, error, bytes.as_bytes(), "stdout") {
        0
    } else {
        1
    }
}
