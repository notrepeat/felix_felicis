//! `universo grafo`: el grafo de un repositorio de criterio (o de un bundle
//! OKF) como una página HTML local e interactiva. Nodos por tipo y estado,
//! aristas por relación, el enlace de promoción entre universos y las marcas
//! de `resumen` (listas para promover, objetadas). Los datos van incrustados y
//! la página no carga nada de la red (CSP `default-src 'none'`). **Su salida
//! incluye datos del bundle**: títulos, atributos y cuerpos.
use super::{criterio::resolver_repo, escribir_salida, resumen::resumir};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use universo_core::Atomo;
use universo_okf::transaccion::{estado::inspeccionar, simulacion::VistaSimulacion};

pub(super) const USO: &str = "Uso: universo grafo [--universo <nombre>] [--criterio <dir>] [--salida <archivo.html>|-] [--ignorar-version]\n       universo grafo <raiz> --manifiesto <m> --invariantes <i> [--salida <archivo.html>|-]\nGenera una página HTML local con el grafo de los universos del repositorio de criterio (o de un bundle OKF): nodos, relaciones, promociones, candidatas listas para promover y objetadas. Muestra todos los universos salvo --universo (FELIX_UNIVERSO no aplica). Sin --salida, la escribe con un nombre nuevo en el directorio temporal. La página incluye datos del bundle y no usa la red.\n";

const PLANTILLA: &str = include_str!("grafo.html");
const MARCA_DATOS: &str = "/*DATOS*/";
/// Cuerpos más largos se recortan en la página.
const MAX_CUERPO: usize = 4000;

enum Origen {
    Criterio {
        criterio: Option<PathBuf>,
        universo: Option<String>,
        ignorar_version: bool,
    },
    Bundle {
        raiz: PathBuf,
        manifiesto: PathBuf,
        invariantes: PathBuf,
    },
}

pub(super) struct Solicitud {
    origen: Origen,
    salida: Option<PathBuf>,
}

pub(super) fn parsear(args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let mut valores: BTreeMap<&str, String> = BTreeMap::new();
    let (mut raiz, mut ignorar_version) = (None, false);
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
        if !arg.starts_with('-') {
            if raiz.replace(PathBuf::from(arg)).is_some() || arg.is_empty() {
                return Err(fail());
            }
            i += 1;
            continue;
        }
        let (nombre, valor, consumidos) = match arg.split_once('=') {
            Some((n, v)) => (n, v.to_string(), 1),
            None => (arg.as_str(), args.get(i + 1).cloned().ok_or_else(fail)?, 2),
        };
        let clave = match nombre {
            "--universo" => "universo",
            "--criterio" => "criterio",
            "--salida" => "salida",
            "--manifiesto" => "manifiesto",
            "--invariantes" => "invariantes",
            _ => return Err(fail()),
        };
        let guion_permitido = clave == "salida" && valor == "-";
        if valor.is_empty() || (valor.starts_with('-') && !guion_permitido) {
            return Err(fail());
        }
        if valores.insert(clave, valor).is_some() {
            return Err(fail());
        }
        i += consumidos;
    }
    let salida = valores.remove("salida").map(PathBuf::from);
    let origen = match raiz {
        Some(raiz) => {
            let (Some(manifiesto), Some(invariantes)) =
                (valores.remove("manifiesto"), valores.remove("invariantes"))
            else {
                return Err(fail());
            };
            if !valores.is_empty() || ignorar_version {
                return Err(fail());
            }
            Origen::Bundle {
                raiz,
                manifiesto: PathBuf::from(manifiesto),
                invariantes: PathBuf::from(invariantes),
            }
        }
        None => {
            if valores.contains_key("manifiesto") || valores.contains_key("invariantes") {
                return Err(fail());
            }
            Origen::Criterio {
                criterio: valores.remove("criterio").map(PathBuf::from),
                universo: valores.remove("universo"),
                ignorar_version,
            }
        }
    };
    Ok(Solicitud { origen, salida })
}

/// Un universo cargado: nombre, estructura y su resumen (para las marcas).
struct Cargado {
    nombre: String,
    vista: VistaSimulacion,
    resumen: Value,
}

fn cargar(
    nombre: &str,
    bundle: &Path,
    manifiesto: &Path,
    invariantes: &Path,
) -> Result<Cargado, String> {
    let vista = VistaSimulacion::capturar(bundle, manifiesto, invariantes)
        .map_err(|e| e.codigo().to_string())?;
    let pendiente = inspeccionar(bundle).map_or(true, |estado| !estado.limpio());
    let resumen = resumir(
        nombre,
        vista.estructura(),
        vista.invariantes().len(),
        bundle,
        pendiente,
    );
    Ok(Cargado {
        nombre: nombre.to_string(),
        vista,
        resumen,
    })
}

/// De dónde vino una nota promovida: la línea que deja `promover` en el
/// criterio de destino, `Promovido desde <universo> (<ruta>) el <fecha>.`.
/// Se busca línea por línea y sólo esa forma exacta cuenta; la ruta puede
/// contener `)`.
fn promovido_desde(cuerpo: &str) -> Option<(&str, &str)> {
    cuerpo.lines().find_map(|linea| {
        let resto = linea.trim().strip_prefix("Promovido desde ")?;
        let (universo, resto) = resto.split_once(" (")?;
        let (ruta, fecha) = resto.rsplit_once(") el ")?;
        let valido = !universo.is_empty()
            && !universo.contains(char::is_whitespace)
            && !ruta.is_empty()
            && fecha.trim_end_matches('.').len() == 10;
        valido.then_some((universo, ruta))
    })
}

fn recortar(texto: &str) -> String {
    if texto.chars().count() <= MAX_CUERPO {
        return texto.to_string();
    }
    let mut corto: String = texto.chars().take(MAX_CUERPO).collect();
    corto.push_str("\n…");
    corto
}

/// Los datos de la página: nodos, aristas y universos.
fn datos(titulo: &str, cargados: &[Cargado]) -> Value {
    let mut nodos = Vec::new();
    let mut aristas = Vec::new();
    // Una relación repetida (multigrafo) se dibuja una vez.
    let mut vistas = BTreeSet::new();
    let mut agregar = |aristas: &mut Vec<Value>, origen: String, destino: String, clave: &str| {
        if vistas.insert((origen.clone(), destino.clone(), clave.to_string())) {
            aristas.push(json!({"origen": origen, "destino": destino, "clave": clave}));
        }
    };
    let mut ids = BTreeSet::new();
    let mut promociones = Vec::new();
    for u in cargados {
        let lista = |clave: &str| -> BTreeSet<String> {
            u.resumen[clave]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.get("path").and_then(Value::as_str).or(x.as_str()))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        let (listas, objetadas) = (lista("ready_to_promote"), lista("objected"));
        for (id, n) in u.vista.estructura().nodos() {
            let path = n.path.to_string();
            let nodo_id = format!("{}/{path}", u.nombre);
            let cuerpo = u
                .vista
                .estructura()
                .anexo(id)
                .map(|a| a.cuerpo.clone())
                .unwrap_or_default();
            if n.tipo.como_str() == "criterio" {
                if let Some((origen, ruta)) = promovido_desde(&cuerpo) {
                    let origen = format!("{origen}/{ruta}");
                    if origen != nodo_id {
                        promociones.push((origen, nodo_id.clone()));
                    }
                }
            }
            let atributos: Map<String, Value> = u
                .vista
                .estructura()
                .atributos(id)
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            let mut marcas = Vec::new();
            if listas.contains(&path) {
                marcas.push("lista_para_promover");
            }
            if objetadas.contains(&path) {
                marcas.push("objetada");
            }
            ids.insert(nodo_id.clone());
            nodos.push(json!({"id": nodo_id, "universo": u.nombre, "path": path,
                "tipo": n.tipo.como_str(), "titulo": n.etiqueta.como_str(),
                "atributos": atributos, "marcas": marcas, "cuerpo": recortar(&cuerpo)}));
        }
        for a in u.vista.estructura().atomos() {
            if let Atomo::Edge {
                origen,
                destino,
                sorte,
                ..
            } = a
            {
                agregar(
                    &mut aristas,
                    format!("{}/{origen}", u.nombre),
                    format!("{}/{destino}", u.nombre),
                    sorte.como_str(),
                );
            }
        }
    }
    for (origen, destino) in promociones {
        if ids.contains(&origen) {
            agregar(&mut aristas, origen, destino, "promovido");
        }
    }
    json!({"schema": "felix-grafo-v1", "titulo": titulo,
           "universos": cargados.iter().map(|u| u.nombre.clone()).collect::<Vec<_>>(),
           "nodos": nodos, "aristas": aristas})
}

/// La página con los datos incrustados. El JSON va en un bloque
/// `application/json`; `<`, `>`, `&` y los separadores de línea Unicode se
/// escapan para que ningún texto del bundle cierre el bloque.
fn pagina(datos: &Value) -> String {
    let json = serde_json::to_string(datos)
        .unwrap_or_default()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    PLANTILLA.replacen(MARCA_DATOS, &json, 1)
}

/// Escribe la página. `nuevo`: sólo si el archivo no existe (en el directorio
/// temporal compartido, así nunca se sigue un enlace ni se reutiliza un
/// archivo ajeno); si no, lo reemplaza (la ruta la eligió el usuario).
fn escribir_archivo(ruta: &Path, contenido: &str, nuevo: bool) -> std::io::Result<()> {
    let mut opciones = fs::OpenOptions::new();
    opciones.write(true);
    if nuevo {
        opciones.create_new(true);
    } else {
        opciones.create(true).truncate(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Datos del bundle: sólo para su dueño, también en el directorio temporal.
        opciones.mode(0o600);
    }
    opciones.open(ruta)?.write_all(contenido.as_bytes())
}

fn error_texto(
    salida: &mut impl Write,
    error: &mut impl Write,
    codigo: &str,
    detalle: &str,
) -> i32 {
    let texto = format!("grafo: error\ncode: {codigo}\n{detalle}");
    escribir_salida(salida, error, texto.as_bytes(), "stdout");
    1
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let (titulo, cargados) = match &s.origen {
        Origen::Criterio {
            criterio,
            universo,
            ignorar_version,
        } => {
            let repo = match resolver_repo(criterio.clone(), *ignorar_version) {
                Ok(repo) => repo,
                Err(e) => return error_texto(salida, error, e.codigo(), &format!("{e}\n")),
            };
            let nombres: Vec<String> = match universo {
                Some(u) => match repo.universo(Some(u)) {
                    Ok(_) => vec![u.clone()],
                    Err(e) => return error_texto(salida, error, e.codigo(), &format!("{e}\n")),
                },
                None => repo.universos.keys().cloned().collect(),
            };
            let mut cargados = Vec::new();
            for nombre in &nombres {
                let Ok(u) = repo.universo(Some(nombre)) else {
                    continue;
                };
                match cargar(nombre, &u.bundle(), &u.manifiesto(), &u.invariantes()) {
                    Ok(c) => cargados.push(c),
                    Err(codigo) => {
                        return error_texto(
                            salida,
                            error,
                            &codigo,
                            &format!("universo: {nombre}\n"),
                        );
                    }
                }
            }
            let titulo = repo.raiz.file_name().map_or_else(
                || "criterio".to_string(),
                |n| n.to_string_lossy().to_string(),
            );
            (titulo, cargados)
        }
        Origen::Bundle {
            raiz,
            manifiesto,
            invariantes,
        } => {
            let nombre = raiz
                .file_name()
                .map_or_else(|| "bundle".to_string(), |n| n.to_string_lossy().to_string());
            match cargar(&nombre, raiz, manifiesto, invariantes) {
                Ok(c) => (nombre, vec![c]),
                Err(codigo) => return error_texto(salida, error, &codigo, ""),
            }
        }
    };
    let html = pagina(&datos(&titulo, &cargados));
    if s.salida.as_deref() == Some(Path::new("-")) {
        return if escribir_salida(salida, error, html.as_bytes(), "stdout") {
            0
        } else {
            1
        };
    }
    let nuevo = s.salida.is_none();
    let destino = s.salida.unwrap_or_else(|| {
        // Nombre único: proceso e instante, y create_new por si acaso.
        let instante = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let seguro: String = titulo
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        std::env::temp_dir().join(format!(
            "felix-grafo-{seguro}-{}-{instante}.html",
            std::process::id()
        ))
    });
    if let Err(e) = escribir_archivo(&destino, &html, nuevo) {
        return error_texto(
            salida,
            error,
            "grafo_io",
            &format!("{}: {e}\n", destino.display()),
        );
    }
    let nodos = cargados
        .iter()
        .map(|c| c.vista.estructura().nodos().count())
        .sum::<usize>();
    let texto = format!(
        "grafo: {}\n{nodos} notas en {} universo(s); ábrelo en un navegador. Incluye datos del bundle: no lo compartas.\n",
        destino.display(),
        cargados.len()
    );
    if escribir_salida(salida, error, texto.as_bytes(), "stdout") {
        0
    } else {
        1
    }
}
