//! Métricas de uso de un repositorio de criterio, compartidas por
//! `universo metricas` y la herramienta `metrics` del MCP. Lo que no se mide no
//! se puede mejorar.
//!
//! Todo sale de lo que Felix ya guarda:
//! - el historial de cada universo, con cada cambio clasificado por su efecto
//!   real: captura, refuerzo, objeción, promoción u otro. Si el autor empieza
//!   por `agente:` es un agente; si no, el dueño;
//! - el estado de las notas: precisión de lo que capturó el agente,
//!   maduración hasta `vigente` y candidatas estancadas;
//! - si `conectar --medir-uso` lo activó, el registro local de llamadas al MCP
//!   (`felix-uso-mcp.jsonl` en el directorio de Git: fecha, herramienta,
//!   universo y si falló, nunca el contenido).
use crate::{
    criterio::RepoCriterio,
    transaccion::{
        historial::{EntradaHistorial, fecha_hoy, leer},
        simulacion::VistaSimulacion,
    },
};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use universo_core::{Atomo, Estructura};

/// Registro local del uso del MCP, dentro del directorio de Git: nunca se versiona.
pub const ARCHIVO_USO_MCP: &str = "felix-uso-mcp.jsonl";
/// Días sin segundo proyecto tras los que una candidata cuenta como estancada.
pub const ESTANCADA_DIAS: i64 = 30;

/// Métricas (`felix-metricas-v1`) de todos los universos de `repo`, con corte
/// en `hasta` (`AAAA-MM-DD`; hoy si es `None`). Sin `incluir_rutas`, la lista
/// de candidatas estancadas sale vacía y sólo queda su número: las rutas son
/// datos del repositorio.
pub fn medir_repositorio(
    repo: &RepoCriterio,
    hasta: Option<&str>,
    estancada_dias: i64,
    incluir_rutas: bool,
) -> Result<Value, String> {
    let mut universos = Vec::new();
    for nombre in repo.universos.keys() {
        let Ok(u) = repo.universo(Some(nombre)) else {
            continue;
        };
        let vista = VistaSimulacion::capturar(&u.bundle(), &u.manifiesto(), &u.invariantes())
            .map_err(|e| e.codigo().to_string())?;
        let entradas = leer(&u.bundle()).map(|h| h.entradas).unwrap_or_default();
        universos.push(Universo {
            nombre: nombre.clone(),
            vista,
            entradas,
        });
    }
    let hoy = match hasta {
        Some(h) => dias(h),
        None => dias(&fecha_hoy()),
    }
    .ok_or_else(|| "fecha_invalida".to_string())?;
    let mut m = medir(&universos, &repo.raiz, hoy, estancada_dias);
    let total = m["estancadas"]["notas"].as_array().map_or(0, Vec::len);
    m["estancadas"]["total"] = json!(total);
    if !incluir_rutas {
        m["estancadas"]["notas"] = json!([]);
    }
    Ok(m)
}

pub fn dias(fecha: &str) -> Option<i64> {
    let b = fecha.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let numero = |r: std::ops::Range<usize>| -> Option<i64> {
        let t = &fecha[r];
        t.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| t.parse().ok())?
    };
    let (y, m, d) = (numero(0..4)?, numero(5..7)?, numero(8..10)?);
    let bisiesto = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let largo = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if bisiesto => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=largo).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// Día de una fecha del historial o del registro de uso (`AAAA-MM-DD` o
/// `AAAA-MM-DDThh:mm:ssZ`).
fn dia_de(fecha_entrada: &str) -> Option<i64> {
    dias(fecha_entrada.get(..10)?)
}

/// `AAAA-MM-DD` de un número de días desde 1970-01-01.
fn fecha(dias: i64) -> String {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Lunes de la semana de una fecha (semanas de lunes a domingo).
fn lunes(fecha_entrada: &str) -> Option<String> {
    let d = dia_de(fecha_entrada)?;
    Some(fecha(d - (d + 3).rem_euclid(7)))
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Accion {
    Captura,
    Refuerzo,
    Objecion,
    Promocion,
    Otro,
}

impl Accion {
    fn nombre(self) -> &'static str {
        match self {
            Self::Captura => "capturas",
            Self::Refuerzo => "refuerzos",
            Self::Objecion => "objeciones",
            Self::Promocion => "promociones",
            Self::Otro => "otros",
        }
    }
}

/// Qué hizo una entrada, por su efecto: una nota de criterio nueva (captura
/// si queda candidata, promoción si llega vigente), una evidencia nueva que
/// apoya (refuerzo) u objeta (objeción), u otra cosa. Devuelve también la
/// nota de criterio creada, si la hay.
fn clasificar(e: &EntradaHistorial) -> (Accion, Option<String>) {
    if e.clase.is_some() {
        return (Accion::Otro, None);
    }
    let Some(efecto) = &e.efecto else {
        return (Accion::Otro, None);
    };
    let mut criterio_nuevo = None;
    let mut estados = BTreeMap::new();
    let (mut apoya, mut objeta, mut evidencia) = (false, false, false);
    for a in &efecto.agregar {
        match a {
            Atomo::Type { sujeto, sorte } if sorte.como_str() == "criterio" => {
                criterio_nuevo = Some(sujeto.to_string());
            }
            Atomo::Type { sorte, .. } if sorte.como_str() == "evidencia" => evidencia = true,
            Atomo::Edge { sorte, .. } if sorte.como_str() == "apoya" => apoya = true,
            Atomo::Edge { sorte, .. } if sorte.como_str() == "objeta" => objeta = true,
            Atomo::Ext {
                predicado,
                sujeto,
                constante,
            } if predicado == "Estado" => {
                estados.insert(sujeto.to_string(), constante.clone());
            }
            _ => {}
        }
    }
    if let Some(nota) = criterio_nuevo {
        let accion = match estados.get(&nota).map(String::as_str) {
            Some("vigente") => Accion::Promocion,
            _ => Accion::Captura,
        };
        return (accion, Some(nota));
    }
    match (evidencia, apoya, objeta) {
        (true, _, true) => (Accion::Objecion, None),
        (true, true, _) => (Accion::Refuerzo, None),
        _ => (Accion::Otro, None),
    }
}

fn es_agente(e: &EntradaHistorial) -> bool {
    e.autor.as_deref().is_some_and(|a| a.starts_with("agente:"))
}

fn atributo(e: &Estructura, nota: &str, predicado: &str) -> Option<String> {
    let id = e.por_path(&universo_core::BundlePath::nuevo(nota).ok()?)?;
    e.atributos(id).get(predicado).cloned()
}

/// Proyectos distintos donde está confirmado cada criterio, y criterios
/// objetados.
fn relaciones(e: &Estructura) -> (BTreeMap<String, BTreeSet<String>>, BTreeSet<String>) {
    let mut proyectos: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut objetados = BTreeSet::new();
    for a in e.atomos() {
        if let Atomo::Edge {
            origen,
            destino,
            sorte,
            ..
        } = a
        {
            match sorte.como_str() {
                "confirmado_en" => {
                    proyectos
                        .entry(origen.to_string())
                        .or_default()
                        .insert(destino.to_string());
                }
                "objeta" => {
                    objetados.insert(destino.to_string());
                }
                _ => {}
            }
        }
    }
    (proyectos, objetados)
}

/// `(fecha de promoción)` de la marca que deja `promover` en el destino.
fn fecha_promocion(cuerpo: &str) -> Option<String> {
    cuerpo.lines().find_map(|l| {
        let l = l.trim();
        l.strip_prefix("Promovido desde ")?;
        let (_, fecha) = l.rsplit_once(") el ")?;
        let fecha = fecha.trim_end_matches('.');
        dias(fecha).map(|_| fecha.to_string())
    })
}

struct Universo {
    nombre: String,
    vista: VistaSimulacion,
    entradas: Vec<EntradaHistorial>,
}

fn mediana(mut v: Vec<i64>) -> Option<i64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[v.len() / 2])
}

fn uso_mcp(raiz: &Path) -> Value {
    let texto = crate::criterio::dir_git(raiz)
        .and_then(|git| std::fs::read_to_string(git.join(ARCHIVO_USO_MCP)).ok());
    let Some(texto) = texto else {
        // Activado por `conectar --medir-uso` pero sin llamadas todavía.
        let activado = std::process::Command::new("git")
            .arg("-C")
            .arg(raiz)
            .args(["config", "--local", "--get", "felix.medirUso"])
            .output()
            .is_ok_and(|o| {
                o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "true"
            });
        if activado {
            return json!({"estado": "sin_llamadas", "llamadas": 0, "errores": 0,
                          "por_herramienta": {}, "por_semana": {}, "descartadas": 0});
        }
        return json!({"estado": "no_medido",
                      "nota": "se activa con universo conectar --aplicar --medir-uso"});
    };
    let (mut llamadas, mut errores, mut descartadas) = (0u64, 0u64, 0u64);
    let mut por_herramienta: BTreeMap<String, u64> = BTreeMap::new();
    let mut por_semana: BTreeMap<String, u64> = BTreeMap::new();
    for linea in texto.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(v) = serde_json::from_str::<Value>(linea) else {
            descartadas += 1;
            continue;
        };
        let (Some(f), Some(h)) = (v["fecha"].as_str(), v["herramienta"].as_str()) else {
            descartadas += 1;
            continue;
        };
        llamadas += 1;
        if v["ok"] == false {
            errores += 1;
        }
        *por_herramienta.entry(h.to_string()).or_default() += 1;
        if let Some(l) = lunes(f) {
            *por_semana.entry(l).or_default() += 1;
        }
    }
    json!({"estado": "medido", "llamadas": llamadas, "errores": errores,
           "por_herramienta": por_herramienta, "por_semana": por_semana, "descartadas": descartadas})
}

fn medir(universos: &[Universo], raiz: &Path, hoy: i64, estancada_dias: i64) -> Value {
    // Actividad: por acción y autor, y por semana.
    let mut actividad: BTreeMap<Accion, (u64, u64)> = BTreeMap::new();
    let mut semanas: BTreeMap<String, BTreeMap<&'static str, u64>> = BTreeMap::new();
    // Criterios de candidatos que capturó un agente.
    let mut del_agente: BTreeSet<String> = BTreeSet::new();
    for u in universos {
        for e in &u.entradas {
            let (accion, nota) = clasificar(e);
            let agente = es_agente(e);
            let fila = actividad.entry(accion).or_default();
            if agente {
                fila.0 += 1;
            } else {
                fila.1 += 1;
            }
            if accion != Accion::Otro {
                if let Some(l) = lunes(&e.fecha) {
                    *semanas
                        .entry(l)
                        .or_default()
                        .entry(accion.nombre())
                        .or_default() += 1;
                }
            }
            if accion == Accion::Captura && agente {
                if let Some(n) = nota {
                    del_agente.insert(format!("{}/{n}", u.nombre));
                }
            }
        }
    }
    let mut act = Map::new();
    for accion in [
        Accion::Captura,
        Accion::Refuerzo,
        Accion::Objecion,
        Accion::Promocion,
        Accion::Otro,
    ] {
        let (agente, dueno) = actividad.get(&accion).copied().unwrap_or_default();
        act.insert(
            accion.nombre().into(),
            json!({"agente": agente, "dueno": dueno, "total": agente + dueno}),
        );
    }
    let por_semana: Vec<Value> = semanas
        .into_iter()
        .map(|(lunes, cuentas)| json!({"semana": lunes, "acciones": cuentas}))
        .collect();

    // Precisión del agente, maduración y estancadas, desde las notas.
    let (mut confirmadas, mut objetadas, mut retiradas, mut pendientes) = (0u64, 0u64, 0u64, 0u64);
    let capturas_agente = del_agente.len();
    let mut vistas = 0usize;
    let mut maduracion = Vec::new();
    let mut estancadas = Vec::new();
    for u in universos {
        let e = u.vista.estructura();
        let (proyectos, objetados) = relaciones(e);
        for (id, n) in e.nodos() {
            if n.tipo.como_str() != "criterio" {
                continue;
            }
            let path = n.path.to_string();
            let estado = atributo(e, &path, "Estado").unwrap_or_default();
            let confirmado = proyectos.get(&path).map_or(0, BTreeSet::len);
            if del_agente.remove(&format!("{}/{path}", u.nombre)) {
                vistas += 1;
                // Promovida cuenta como confirmada aunque alguna vez la
                // hayan objetado: el dueño decidió.
                if estado == "promovido" || estado == "vigente" {
                    confirmadas += 1;
                } else if objetados.contains(&path) {
                    objetadas += 1;
                } else if confirmado >= 2 {
                    confirmadas += 1;
                } else if estado == "retirado" {
                    retiradas += 1;
                } else {
                    pendientes += 1;
                }
            }
            if estado == "vigente" {
                let cuerpo = e.anexo(id).map(|a| a.cuerpo.as_str()).unwrap_or_default();
                if let (Some(captura), Some(promocion)) = (
                    atributo(e, &path, "Fecha").as_deref().and_then(dias),
                    fecha_promocion(cuerpo).as_deref().and_then(dias),
                ) {
                    maduracion.push(promocion - captura);
                }
            }
            if estado == "candidato" && confirmado < 2 && !objetados.contains(&path) {
                if let Some(f) = atributo(e, &path, "Fecha").as_deref().and_then(dias) {
                    if hoy - f > estancada_dias {
                        estancadas.push(path.clone());
                    }
                }
            }
        }
    }
    let resueltas = confirmadas + objetadas + retiradas;
    let precision =
        (resueltas > 0).then(|| (confirmadas as f64 / resueltas as f64 * 1000.0).round() / 1000.0);
    estancadas.sort();
    json!({
        "schema": "felix-metricas-v1",
        "hasta": fecha(hoy),
        "universos": universos.iter().map(|u| json!({"nombre": u.nombre,
            "notas": u.vista.estructura().nodos().count(), "cambios": u.entradas.len()}))
            .collect::<Vec<_>>(),
        "actividad": act,
        "por_semana": por_semana,
        "precision_agente": {"capturas": capturas_agente, "confirmadas": confirmadas,
            "desaparecidas": capturas_agente - vistas,
            "objetadas": objetadas, "retiradas": retiradas, "pendientes": pendientes,
            "precision": precision,
            "definicion": "confirmadas / (confirmadas + objetadas + retiradas); confirmada = reforzada en dos proyectos o promovida"},
        "maduracion": {"promovidas": maduracion.len(), "dias_mediana": mediana(maduracion.clone()),
            "dias_max": maduracion.iter().max()},
        "estancadas": {"dias": estancada_dias, "notas": estancadas},
        "uso_mcp": uso_mcp(raiz),
    })
}

#[cfg(test)]
mod tests {
    use super::{dia_de, dias, fecha, lunes};

    #[test]
    fn fechas_y_semanas() {
        assert_eq!(dias("1970-01-01"), Some(0));
        assert_eq!(fecha(dias("2026-11-19").unwrap()), "2026-11-19");
        assert_eq!(fecha(dias("2024-02-29").unwrap()), "2024-02-29");
        assert_eq!(dias("2026-13-01"), None);
        assert_eq!(dias("2026-02-31"), None);
        assert_eq!(dias("2023-02-29"), None);
        assert_eq!(dias("2026-11-19xyz"), None);
        assert_eq!(fecha(dias("2000-02-29").unwrap()), "2000-02-29");
        assert_eq!(dias("1900-02-29"), None);
        assert_eq!(fecha(dias("1969-12-31").unwrap()), "1969-12-31");
        assert_eq!(dias("1969-12-31"), Some(-1));
        assert_eq!(dia_de("2026-11-19T10:00:00Z"), dias("2026-11-19"));
        assert_eq!(dias("ayer"), None);
        // 2026-11-19 es jueves: su semana empieza el lunes 16.
        assert_eq!(lunes("2026-11-19T10:00:00Z").as_deref(), Some("2026-11-16"));
        assert_eq!(lunes("2026-11-16").as_deref(), Some("2026-11-16"));
        assert_eq!(lunes("2026-11-22").as_deref(), Some("2026-11-16"));
    }
}
