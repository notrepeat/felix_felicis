//! `universo capturar`, `reforzar` y `objetar`: guardar criterio en un comando
//! (fase 10, U3 y U12).
//!
//! Cada comando arma un delta `felix-delta-v2`, lo simula y, si se admite, lo
//! aplica con CAS sobre el estado simulado, con autor y motivo en el historial.
//! Usan el **esquema de captura v1**, que `universo init --git` deja en cada
//! universo: tipos `criterio`, `evidencia` y `proyecto`; aristas
//! `confirmado_en` (criterio → proyecto), `apoya` y `objeta` (evidencia →
//! criterio) y `en_proyecto` (evidencia → proyecto); atributos `estado`,
//! `validacion` y `fecha`; directorios `criterios`, `evidencias` y
//! `proyectos`. **Su salida incluye datos del bundle** (rutas y títulos).
use super::{Formato, escribir_salida, historial::visible};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
};
use universo_core::{Atomo, BundlePath, Estructura, ResultadoPropuesta};
use universo_okf::{
    busqueda::normalizar,
    parsear_delta_json,
    transaccion::{
        aplicar::{EstadoAplicacion, aplicar_archivo_con_procedencia},
        historial::{Procedencia, fecha_hoy},
        simulacion::VistaSimulacion,
    },
};

pub(super) const USO: &str = "Uso:\n  universo capturar <texto> --proyecto <nombre> [--tarea <t>] [--validacion explicita|implicita|pendiente] [--fecha AAAA-MM-DD, por defecto la fecha UTC de hoy] [--autor <a>] [--motivo <m>] [--forzar] [--dry-run] [--format text|json]\n  universo reforzar <nota> --proyecto <nombre> --contexto <c> [--validacion ...] [--fecha ...] [--autor <a>] [--motivo <m>] [--dry-run] [--format text|json]\n  universo objetar <nota> --proyecto <nombre> --contexto <c> [--validacion ...] [--fecha ...] [--autor <a>] [--motivo <m>] [--dry-run] [--format text|json]\nDentro de un repositorio de criterio no hacen falta la raíz ni --manifiesto/--invariantes.\n";

/// Qué se está guardando.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Accion {
    Capturar,
    Reforzar,
    Objetar,
}

impl Accion {
    fn nombre(self) -> &'static str {
        match self {
            Self::Capturar => "capturar",
            Self::Reforzar => "reforzar",
            Self::Objetar => "objetar",
        }
    }
}

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

const VALIDACIONES: [&str; 3] = ["explicita", "implicita", "pendiente"];
const MAX_TEXTO: usize = 300;
const MAX_PROYECTO: usize = 100;
const MAX_DETALLE: usize = 2000;

/// Una línea imprimible, sin controles ni caracteres invisibles.
pub(super) fn linea_valida(texto: &str, maximo: usize) -> bool {
    !texto.trim().is_empty()
        && texto.chars().count() <= maximo
        && !texto
            .chars()
            .any(|c| c.is_control() || universo_core::sexpr::caracter_no_imprimible(c))
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

/// Palabras normalizadas (sin tildes, minúsculas) unidas por guiones, hasta
/// 60 caracteres sin cortar una palabra.
pub(super) fn slug(texto: &str) -> String {
    let mut salida = String::new();
    for palabra in normalizar(texto)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|p| !p.is_empty())
    {
        let palabra = &palabra[..palabra.len().min(60)];
        if !salida.is_empty() && salida.len() + 1 + palabra.len() > 60 {
            break;
        }
        if !salida.is_empty() {
            salida.push('-');
        }
        salida.push_str(palabra);
    }
    salida
}

/// Nombres que Windows reserva o que el motor trata aparte (`index`).
const RESERVADOS: &[&str] = &[
    "index", "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
    "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Nombre base de una nota: el slug, con sufijo si está reservado.
fn base_de_nota(slug: String) -> String {
    if RESERVADOS.contains(&slug.as_str()) {
        format!("{slug}-nota")
    } else {
        slug
    }
}

/// `prefijo-slug`, o sólo `prefijo` si el texto no deja slug (por ejemplo,
/// un nombre sin letras latinas).
fn con_prefijo(prefijo: &str, texto: &str) -> String {
    let slug = slug(texto);
    if slug.is_empty() {
        prefijo.to_string()
    } else {
        format!("{prefijo}-{slug}")
    }
}

/// Error de dominio (salida 1) con su código y, si hace falta, un detalle.
pub(super) struct Fallo {
    pub codigo: &'static str,
    pub detalle: Value,
}

pub(super) fn fallo(codigo: &'static str) -> Fallo {
    Fallo {
        codigo,
        detalle: Value::Null,
    }
}

/// Lo que falta del esquema de captura en el manifiesto del universo.
fn esquema_faltante(e: &Estructura, accion: Accion) -> Vec<String> {
    let m = e.manifiesto();
    let mut falta = Vec::new();
    let tipos: &[&str] = match accion {
        Accion::Capturar => &["criterio", "proyecto"],
        _ => &["criterio", "evidencia", "proyecto"],
    };
    for tipo in tipos {
        if !m.sortes_nodo.iter().any(|s| s.como_str() == *tipo) {
            falta.push(format!("tipo {tipo}"));
        }
    }
    let aristas: &[&str] = match accion {
        Accion::Capturar => &["confirmado_en"],
        Accion::Reforzar => &["confirmado_en", "apoya", "en_proyecto"],
        Accion::Objetar => &["objeta", "en_proyecto"],
    };
    for clave in aristas {
        if !m.aristas.iter().any(|a| a.clave == *clave) {
            falta.push(format!("arista {clave}"));
        }
    }
    let predicados: &[(&str, &str)] = match accion {
        Accion::Capturar => &[("Estado", "candidato"), ("Validacion", ""), ("Fecha", "")],
        _ => &[("Validacion", ""), ("Fecha", "")],
    };
    for (nombre, valor) in predicados {
        let ok = m.predicados.iter().any(|p| {
            p.nombre == *nombre
                && (valor.is_empty() || p.dominio.contains(*valor))
                && (*nombre != "Fecha" || p.fecha)
        });
        if !ok {
            falta.push(format!("predicado {nombre}"));
        }
    }
    let directorios: &[&str] = match accion {
        Accion::Capturar => &["criterios", "proyectos"],
        _ => &["evidencias", "proyectos"],
    };
    for dir in directorios {
        if !m.layout.directorios.iter().any(|d| d == dir) {
            falta.push(format!("directorio {dir}"));
        }
    }
    falta
}

/// Ruta libre `dir/base`, `dir/base-2`, … cuyo nombre (stem) no repita el de
/// ninguna nota: los enlaces del frontmatter se resuelven por nombre.
pub(super) fn ruta_libre(
    e: &Estructura,
    dir: &str,
    base: &str,
    reservados: &BTreeSet<String>,
) -> String {
    // Como el motor, sin distinguir mayúsculas.
    let stems: BTreeSet<String> = e
        .nodos()
        .map(|(_, n)| n.path.stem().to_lowercase())
        .collect();
    let mut n = 1;
    loop {
        let stem = if n == 1 {
            base.to_string()
        } else {
            format!("{base}-{n}")
        };
        let clave = stem.to_lowercase();
        if !stems.contains(&clave) && !reservados.contains(&clave) {
            return format!("{dir}/{stem}");
        }
        n += 1;
    }
}

pub(super) fn es_arista(a: &Atomo, origen: &str, destino: &str, clave: &str) -> bool {
    matches!(a, Atomo::Edge { origen: o, destino: d, sorte, .. }
        if o.como_str() == origen && d.como_str() == destino && sorte.como_str() == clave)
}

/// Una arista nueva `origen → destino` de clave `clave`, con su etiqueta vacía.
pub(super) fn arista(e: &Estructura, origen: &str, destino: &str, clave: &str) -> [String; 2] {
    let ordinal = e
        .atomos()
        .iter()
        .filter(|a| es_arista(a, origen, destino, clave))
        .count();
    let id = format!("{origen}→{destino}#{clave}#{ordinal}");
    [
        format!("Edge({id}, {origen}, {destino}, {clave})"),
        format!("HasLab({id}, \"\")"),
    ]
}

/// Proyecto con ese nombre (sin distinguir mayúsculas ni tildes), o la ruta
/// que tendrá si hay que crearlo.
pub(super) fn proyecto(
    e: &Estructura,
    nombre: &str,
    reservados: &BTreeSet<String>,
) -> (String, bool) {
    // «tienda-web» y «Tienda Web» son el mismo proyecto: se compara también
    // por slug, que ignora mayúsculas, tildes, espacios y puntuación.
    let buscado = normalizar(nombre.trim());
    let buscado_slug = slug(nombre);
    if let Some((_, nodo)) = e.nodos().find(|(_, n)| {
        let titulo = n.etiqueta.como_str();
        n.tipo.como_str() == "proyecto"
            && (normalizar(titulo.trim()) == buscado
                || (!buscado_slug.is_empty() && slug(titulo) == buscado_slug))
    }) {
        return (nodo.path.to_string(), false);
    }
    (
        ruta_libre(e, "proyectos", &con_prefijo("proyecto", nombre), reservados),
        true,
    )
}

fn stem(ruta: &str) -> String {
    ruta.rsplit('/').next().unwrap_or_default().to_lowercase()
}

/// Lo que se va a escribir, ya armado.
struct Plan {
    delta: Value,
    nota: String,
    proyecto: String,
    proyecto_creado: bool,
    evidencia: Option<String>,
}

fn planificar(s: &Solicitud, e: &Estructura) -> Result<Plan, Fallo> {
    let fecha = s.fecha.clone().unwrap_or_else(fecha_hoy);
    let mut reservados = BTreeSet::new();
    let mut crear = Vec::new();
    let mut agregar = Vec::new();
    let contexto = {
        let mut texto = format!(
            "## Contexto\n\n- Proyecto: {}\n- Fecha: {fecha}\n- Validación: {}\n",
            s.proyecto.trim(),
            s.validacion
        );
        if let Some(detalle) = &s.detalle {
            let etiqueta = if s.accion == Accion::Capturar {
                "Tarea"
            } else {
                "Contexto"
            };
            texto.push_str(&format!("- {etiqueta}: {}\n", detalle.trim()));
        }
        texto
    };
    let (nota, evidencia) = match s.accion {
        Accion::Capturar => {
            let slug_texto = slug(&s.objetivo);
            if slug_texto.is_empty() {
                return Err(fallo("capture_text_invalid"));
            }
            let base = base_de_nota(slug_texto.clone());
            // La misma afirmación ya capturada se refuerza, no se duplica.
            let buscado = normalizar(s.objetivo.trim());
            let parecidas: Vec<String> = e
                .nodos()
                .filter(|(_, n)| {
                    n.tipo.como_str() == "criterio"
                        && (normalizar(n.etiqueta.como_str().trim()) == buscado
                            || slug(n.etiqueta.como_str()) == slug_texto)
                })
                .map(|(_, n)| n.path.to_string())
                .collect();
            if !parecidas.is_empty() && !s.forzar {
                return Err(Fallo {
                    codigo: "capture_similar",
                    detalle: json!(parecidas),
                });
            }
            let nota = ruta_libre(e, "criterios", &base, &reservados);
            reservados.insert(stem(&nota));
            crear.push(
                json!({"path": nota, "type": "criterio", "title": s.objetivo.trim(),
                              "body": format!("{}\n\n{contexto}", s.objetivo.trim())}),
            );
            agregar.push(format!("Estado({nota}, candidato)"));
            agregar.push(format!("Validacion({nota}, {})", s.validacion));
            agregar.push(format!("Fecha({nota}, {fecha})"));
            (nota, None)
        }
        Accion::Reforzar | Accion::Objetar => {
            let ruta = BundlePath::nuevo(&s.objetivo).map_err(|_| fallo("note_not_found"))?;
            let criterio = e
                .por_path(&ruta)
                .and_then(|id| e.nodo(id))
                .filter(|n| n.tipo.como_str() == "criterio")
                .ok_or_else(|| fallo("note_not_found"))?;
            let titulo = criterio.etiqueta.como_str().to_string();
            let evidencia = ruta_libre(
                e,
                "evidencias",
                &con_prefijo(&format!("evidencia-{fecha}"), &titulo),
                &reservados,
            );
            reservados.insert(stem(&evidencia));
            let prefijo = if s.accion == Accion::Reforzar {
                "Refuerzo"
            } else {
                "Objeción"
            };
            crear.push(json!({"path": evidencia, "type": "evidencia",
                              "title": format!("{prefijo}: {titulo} ({})", s.proyecto.trim()),
                              "body": contexto}));
            agregar.push(format!("Validacion({evidencia}, {})", s.validacion));
            agregar.push(format!("Fecha({evidencia}, {fecha})"));
            (ruta.to_string(), Some(evidencia))
        }
    };
    let (proyecto, creado) = proyecto(e, &s.proyecto, &reservados);
    if creado {
        crear.push(json!({"path": proyecto, "type": "proyecto", "title": s.proyecto.trim()}));
    }
    match (&evidencia, s.accion) {
        (None, _) => agregar.extend(arista(e, &nota, &proyecto, "confirmado_en")),
        (Some(evidencia), accion) => {
            let clave = if accion == Accion::Reforzar {
                "apoya"
            } else {
                "objeta"
            };
            agregar.extend(arista(e, evidencia, &nota, clave));
            agregar.extend(arista(e, evidencia, &proyecto, "en_proyecto"));
            // Un refuerzo confirma el criterio en ese proyecto, una sola vez.
            let ya = e
                .atomos()
                .iter()
                .any(|a| es_arista(a, &nota, &proyecto, "confirmado_en"));
            if accion == Accion::Reforzar && !ya {
                agregar.extend(arista(e, &nota, &proyecto, "confirmado_en"));
            }
        }
    }
    Ok(Plan {
        delta: json!({"schema":"felix-delta-v2","create":crear,"update":[],"add":agregar,"remove":[]}),
        nota,
        proyecto,
        proyecto_creado: creado,
        evidencia,
    })
}

/// El delta en un archivo temporal propio, que se borra al terminar.
struct ArchivoTemporal(PathBuf);

impl ArchivoTemporal {
    fn crear(contenido: &str) -> std::io::Result<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let ruta =
            std::env::temp_dir().join(format!("felix-captura-{}-{nanos}.json", std::process::id()));
        let mut opciones = std::fs::OpenOptions::new();
        opciones.write(true).create_new(true);
        // Contiene el texto del criterio: sólo lo lee su dueño.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opciones.mode(0o600);
        }
        let mut archivo = opciones.open(&ruta)?;
        let escrito = archivo.write_all(contenido.as_bytes());
        let temporal = Self(ruta);
        escrito.map(|()| temporal)
    }
    fn ruta(&self) -> &Path {
        &self.0
    }
}

impl Drop for ArchivoTemporal {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
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

/// Rutas de un universo.
pub(super) struct Rutas<'a> {
    pub raiz: &'a Path,
    pub manifiesto: &'a Path,
    pub invariantes: &'a Path,
}

/// Veredicto de simular un delta y, si se admite y no es `--dry-run`, aplicarlo.
pub(super) enum Veredicto {
    Admitido {
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

/// Simula `delta` sobre `vista` y, si se admite, lo aplica con CAS sobre el
/// mismo estado (el token de la vista), con autor y motivo en el historial.
pub(super) fn simular_y_aplicar(
    vista: &VistaSimulacion,
    rutas: &Rutas<'_>,
    delta: &Value,
    procedencia: &Procedencia,
    dry_run: bool,
) -> Result<Veredicto, Fallo> {
    let texto = delta.to_string();
    let tipado = parsear_delta_json(&texto).map_err(|_| fallo("delta_invalid"))?;
    match vista.proponer(&tipado).map_err(|e| fallo(e.codigo()))? {
        ResultadoPropuesta::Admitida(_) => {}
        ResultadoPropuesta::RechazadaConflictos(conflictos) => {
            return Ok(Veredicto::Rechazado {
                reglas: Vec::new(),
                conflictos: conflictos.total_conflictos(),
            });
        }
        ResultadoPropuesta::RechazadaInvariantes(fuentes) => {
            let reglas = vista.invariantes();
            return Ok(Veredicto::Rechazado {
                reglas: fuentes
                    .iter()
                    .map(|p| {
                        let f = reglas.get(p.source_index);
                        json!({"rule_id": f.and_then(|f| f.id.clone()),
                               "rule_message": f.and_then(|f| f.mensaje.clone())})
                    })
                    .collect(),
                conflictos: 0,
            });
        }
    }
    if dry_run {
        return Ok(Veredicto::Admitido {
            aplicado: false,
            sellado: false,
            registrado: false,
        });
    }
    let archivo = ArchivoTemporal::crear(&texto).map_err(|_| fallo("capture_io"))?;
    let resultado = aplicar_archivo_con_procedencia(
        rutas.raiz,
        rutas.manifiesto,
        rutas.invariantes,
        archivo.ruta(),
        vista.snapshot_token(),
        procedencia,
    )
    .map_err(|e| Fallo {
        codigo: e.codigo(),
        detalle: json!(["si quedó una transacción a medias, `universo status` lo dice y `universo recover` la completa"]),
    })?;
    match resultado.estado {
        EstadoAplicacion::Aplicada => Ok(Veredicto::Admitido {
            aplicado: true,
            sellado: resultado.sellado,
            registrado: resultado.registrado,
        }),
        // Un cambio de estado entre simular y aplicar lo rechaza el CAS como
        // error; un veredicto distinto sobre el mismo estado no debería darse.
        EstadoAplicacion::Rechazada => Err(fallo("capture_rejected_on_apply")),
    }
}

fn correr(s: &Solicitud) -> Result<Resultado, Fallo> {
    let vista = VistaSimulacion::capturar(&s.raiz, &s.manifiesto, &s.invariantes)
        .map_err(|e| fallo(e.codigo()))?;
    let falta = esquema_faltante(vista.estructura(), s.accion);
    if !falta.is_empty() {
        return Err(Fallo {
            codigo: "capture_schema_missing",
            detalle: json!(falta),
        });
    }
    let plan = planificar(s, vista.estructura())?;
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
    Ok(
        match simular_y_aplicar(&vista, &rutas, &plan.delta, &procedencia, s.dry_run)? {
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
            Veredicto::Rechazado { reglas, conflictos } => {
                Resultado::Rechazado { reglas, conflictos }
            }
        },
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_sin_tildes_ni_simbolos_y_acotado() {
        assert_eq!(
            slug("Los componentes deben ser 100 % reutilizables"),
            "los-componentes-deben-ser-100-reutilizables"
        );
        assert_eq!(
            slug("¿Validar antes de crear? ¡Sí!"),
            "validar-antes-de-crear-si"
        );
        assert!(slug(&"palabra ".repeat(30)).len() <= 60);
        assert_eq!(slug("%%%"), "");
    }
}
