//! Registro de criterio (fase 10): capturar, reforzar y objetar, compartido
//! por la CLI (`universo capturar|reforzar|objetar`) y el MCP.
//!
//! Cada operación arma un delta `felix-delta-v2`, lo simula y, si se admite,
//! lo aplica con CAS sobre el estado simulado, con autor y motivo en el
//! historial. Usan el **esquema de captura v1** que `universo init --git` deja
//! en cada universo: tipos `criterio`, `evidencia` y `proyecto`; aristas
//! `confirmado_en` (criterio → proyecto), `apoya` y `objeta` (evidencia →
//! criterio) y `en_proyecto` (evidencia → proyecto); atributos `estado`,
//! `validacion` y `fecha`; directorios `criterios`, `evidencias` y `proyectos`.
use crate::{
    busqueda::normalizar,
    parsear_delta_json,
    transaccion::{
        aplicar::{EstadoAplicacion, aplicar_archivo_con_procedencia},
        historial::{Procedencia, fecha_hoy},
        simulacion::VistaSimulacion,
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::Write,
    path::{Path, PathBuf},
};
use universo_core::{Atomo, BundlePath, Estructura, ResultadoPropuesta};

/// Esquema de captura v1 (fase 10): el que usan `capturar`, `reforzar` y `objetar`.
pub const MANIFIESTO_CRITERIO: &str = r#"# Esquema de captura v1 (fase 10).
version = "criterio/1"

[nodos]
sortes = ["criterio", "evidencia", "proyecto", "Retirado", "Patrón"]

[[aristas]]
clave = "confirmado_en"

[[aristas]]
clave = "apoya"

[[aristas]]
clave = "objeta"

[[aristas]]
clave = "en_proyecto"

[[predicados]]
nombre = "Estado"
clave = "estado"
dominio = ["candidato", "promovido", "vigente", "retirado"]

[[predicados]]
nombre = "Validacion"
clave = "validacion"
dominio = ["explicita", "implicita", "pendiente"]

[[predicados]]
nombre = "Fecha"
clave = "fecha"
tipo = "fecha"

[layout]
directorios = ["criterios", "evidencias", "proyectos"]
"#;

/// Reglas de `candidatos`: toda nota con su contexto y aristas bien tipadas.
pub const INVARIANTES_CANDIDATOS: &str = r#"; Reglas de candidatos creadas por `universo init --git`.
(regla :id "tipo-conocido" :mensaje "Cada nota es un criterio, una evidencia o un proyecto."
  (forall ((x N))
    (or (Type x criterio) (Type x evidencia) (Type x proyecto))))

(regla :id "criterio-con-contexto" :mensaje "Todo criterio tiene estado, fecha, validación y al menos un proyecto donde se expresó."
  (forall ((x N))
    (=> (Type x criterio)
        (and (or (Estado x candidato) (Estado x promovido) (Estado x vigente) (Estado x retirado))
             (>= Fecha x 1900-01-01)
             (or (Validacion x explicita) (Validacion x implicita) (Validacion x pendiente))
             (al-menos 1 ((e E) (y N)) :guarda (Edge e x y confirmado_en) (Type y proyecto))))))

(regla :id "evidencia-con-contexto" :mensaje "Toda evidencia tiene fecha, un proyecto y apoya u objeta un criterio."
  (forall ((x N))
    (=> (Type x evidencia)
        (and (>= Fecha x 1900-01-01)
             (al-menos 1 ((e E) (y N)) :guarda (Edge e x y en_proyecto) (Type y proyecto))
             (or (al-menos 1 ((e E) (y N)) :guarda (Edge e x y apoya) (Type y criterio))
                 (al-menos 1 ((e E) (y N)) :guarda (Edge e x y objeta) (Type y criterio)))))))

(regla :id "aristas-bien-tipadas" :mensaje "confirmado_en va de un criterio a un proyecto; apoya y objeta, de una evidencia a un criterio; en_proyecto, de una evidencia a un proyecto."
  (and
    (forall ((e E) (x N) (y N)) :guarda (Edge e x y confirmado_en) (and (Type x criterio) (Type y proyecto)))
    (forall ((e E) (x N) (y N)) :guarda (Edge e x y apoya) (and (Type x evidencia) (Type y criterio)))
    (forall ((e E) (x N) (y N)) :guarda (Edge e x y objeta) (and (Type x evidencia) (Type y criterio)))
    (forall ((e E) (x N) (y N)) :guarda (Edge e x y en_proyecto) (and (Type x evidencia) (Type y proyecto)))))
"#;

/// Reglas de `vigente`: las de candidatos y, además, confirmación en dos proyectos.
pub const INVARIANTES_VIGENTE: &str = r#"; Reglas de vigente creadas por `universo init --git`.
(regla :id "tipo-conocido" :mensaje "Cada nota es un criterio, una evidencia o un proyecto."
  (forall ((x N))
    (or (Type x criterio) (Type x evidencia) (Type x proyecto))))

(regla :id "criterio-con-contexto" :mensaje "Todo criterio tiene estado, fecha, validación y al menos un proyecto donde se expresó."
  (forall ((x N))
    (=> (Type x criterio)
        (and (or (Estado x candidato) (Estado x promovido) (Estado x vigente) (Estado x retirado))
             (>= Fecha x 1900-01-01)
             (or (Validacion x explicita) (Validacion x implicita) (Validacion x pendiente))
             (al-menos 1 ((e E) (y N)) :guarda (Edge e x y confirmado_en) (Type y proyecto))))))

(regla :id "evidencia-con-contexto" :mensaje "Toda evidencia tiene fecha, un proyecto y apoya u objeta un criterio."
  (forall ((x N))
    (=> (Type x evidencia)
        (and (>= Fecha x 1900-01-01)
             (al-menos 1 ((e E) (y N)) :guarda (Edge e x y en_proyecto) (Type y proyecto))
             (or (al-menos 1 ((e E) (y N)) :guarda (Edge e x y apoya) (Type y criterio))
                 (al-menos 1 ((e E) (y N)) :guarda (Edge e x y objeta) (Type y criterio)))))))

(regla :id "aristas-bien-tipadas" :mensaje "confirmado_en va de un criterio a un proyecto; apoya y objeta, de una evidencia a un criterio; en_proyecto, de una evidencia a un proyecto."
  (and
    (forall ((e E) (x N) (y N)) :guarda (Edge e x y confirmado_en) (and (Type x criterio) (Type y proyecto)))
    (forall ((e E) (x N) (y N)) :guarda (Edge e x y apoya) (and (Type x evidencia) (Type y criterio)))
    (forall ((e E) (x N) (y N)) :guarda (Edge e x y objeta) (and (Type x evidencia) (Type y criterio)))
    (forall ((e E) (x N) (y N)) :guarda (Edge e x y en_proyecto) (and (Type x evidencia) (Type y proyecto)))))

(regla :id "solo-criterio-probado" :mensaje "En vigente, un criterio está vigente o retirado: las candidatas viven en candidatos."
  (forall ((x N))
    (=> (Type x criterio) (or (Estado x vigente) (Estado x retirado)))))

(regla :id "vigente-confirmado" :mensaje "Un criterio vigente está confirmado en al menos dos proyectos."
  (forall ((x N))
    (=> (Estado x vigente)
        (al-menos 2 ((e E) (y N)) :guarda (Edge e x y confirmado_en) (Type y proyecto)))))
"#;

/// Qué guardar y en qué contexto; la validación de los campos la hace quien
/// la arma (ver [`linea_valida`], [`VALIDACIONES`] y los `MAX_*`).
#[derive(Clone, Debug)]
pub struct Peticion {
    pub accion: Accion,
    /// `Capturar`: la afirmación; `Reforzar`/`Objetar`: la ruta de la nota.
    pub objetivo: String,
    pub proyecto: String,
    /// `Capturar`: la tarea; `Reforzar`/`Objetar`: el contexto.
    pub detalle: Option<String>,
    pub validacion: String,
    /// `AAAA-MM-DD`; por defecto, la fecha UTC de hoy.
    pub fecha: Option<String>,
    /// Capturar aunque exista un criterio con el mismo título.
    pub forzar: bool,
}

/// Qué se está guardando.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accion {
    Capturar,
    Reforzar,
    Objetar,
}

impl Accion {
    pub fn nombre(self) -> &'static str {
        match self {
            Self::Capturar => "capturar",
            Self::Reforzar => "reforzar",
            Self::Objetar => "objetar",
        }
    }
}

pub const VALIDACIONES: [&str; 3] = ["explicita", "implicita", "pendiente"];

pub const MAX_TEXTO: usize = 300;

pub const MAX_PROYECTO: usize = 100;

pub const MAX_DETALLE: usize = 2000;

/// Una línea imprimible, sin controles ni caracteres invisibles.
pub fn linea_valida(texto: &str, maximo: usize) -> bool {
    !texto.trim().is_empty()
        && texto.chars().count() <= maximo
        && !texto
            .chars()
            .any(|c| c.is_control() || universo_core::sexpr::caracter_no_imprimible(c))
}

/// Palabras normalizadas (sin tildes, minúsculas) unidas por guiones, hasta
/// 60 caracteres sin cortar una palabra.
pub fn slug(texto: &str) -> String {
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
pub fn base_de_nota(slug: String) -> String {
    if RESERVADOS.contains(&slug.as_str()) {
        format!("{slug}-nota")
    } else {
        slug
    }
}

/// `prefijo-slug`, o sólo `prefijo` si el texto no deja slug (por ejemplo,
/// un nombre sin letras latinas).
pub fn con_prefijo(prefijo: &str, texto: &str) -> String {
    let slug = slug(texto);
    if slug.is_empty() {
        prefijo.to_string()
    } else {
        format!("{prefijo}-{slug}")
    }
}

/// Error de dominio (salida 1) con su código y, si hace falta, un detalle.
pub struct Fallo {
    pub codigo: &'static str,
    pub detalle: Value,
}

pub fn fallo(codigo: &'static str) -> Fallo {
    Fallo {
        codigo,
        detalle: Value::Null,
    }
}

/// Lo que falta del esquema de captura en el manifiesto del universo.
pub fn esquema_faltante(e: &Estructura, accion: Accion) -> Vec<String> {
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
pub fn ruta_libre(e: &Estructura, dir: &str, base: &str, reservados: &BTreeSet<String>) -> String {
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

pub fn es_arista(a: &Atomo, origen: &str, destino: &str, clave: &str) -> bool {
    matches!(a, Atomo::Edge { origen: o, destino: d, sorte, .. }
        if o.como_str() == origen && d.como_str() == destino && sorte.como_str() == clave)
}

/// Una arista nueva `origen → destino` de clave `clave`, con su etiqueta vacía.
pub fn arista(e: &Estructura, origen: &str, destino: &str, clave: &str) -> [String; 2] {
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
pub fn proyecto(e: &Estructura, nombre: &str, reservados: &BTreeSet<String>) -> (String, bool) {
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
pub struct Plan {
    pub delta: Value,
    pub nota: String,
    pub proyecto: String,
    pub proyecto_creado: bool,
    pub evidencia: Option<String>,
}

pub fn planificar(s: &Peticion, e: &Estructura) -> Result<Plan, Fallo> {
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

/// Rutas de un universo.
pub struct Rutas<'a> {
    pub raiz: &'a Path,
    pub manifiesto: &'a Path,
    pub invariantes: &'a Path,
}

/// Veredicto de simular un delta y, si se admite y no es `--dry-run`, aplicarlo.
pub enum Veredicto {
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
pub fn simular_y_aplicar(
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

/// Captura, refuerzo u objeción completos: lee el universo, comprueba el
/// esquema, arma el delta y lo simula y aplica (o sólo simula, con `dry_run`).
pub fn registrar(
    rutas: &Rutas<'_>,
    peticion: &Peticion,
    procedencia: &Procedencia,
    dry_run: bool,
) -> Result<(Plan, Veredicto), Fallo> {
    let vista = VistaSimulacion::capturar(rutas.raiz, rutas.manifiesto, rutas.invariantes)
        .map_err(|e| fallo(e.codigo()))?;
    let falta = esquema_faltante(vista.estructura(), peticion.accion);
    if !falta.is_empty() {
        return Err(Fallo {
            codigo: "capture_schema_missing",
            detalle: json!(falta),
        });
    }
    let plan = planificar(peticion, vista.estructura())?;
    let veredicto = simular_y_aplicar(&vista, rutas, &plan.delta, procedencia, dry_run)?;
    Ok((plan, veredicto))
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
