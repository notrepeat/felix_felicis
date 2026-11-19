//! `universo relacion`, `atributo` y `nota editar` (fase 10, U3): cambios
//! puntuales en un universo, cada uno en un comando. Arman el delta, lo
//! simulan y lo aplican con CAS, con autor y motivo, como `capturar`.
//! **Su salida incluye datos del bundle** (rutas y valores).
use super::{
    Formato,
    captura_criterio::{Fallo, Rutas, Veredicto, arista, es_arista, fallo, simular_y_aplicar},
    escribir_salida,
    historial::visible,
};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use universo_core::{Atomo, BundlePath, Estructura, Sujeto};
use universo_okf::{
    cargar_manifiesto,
    delta::serializar_delta_json_canonico,
    delta_desde_borrador,
    transaccion::{historial::Procedencia, simulacion::VistaSimulacion},
};

pub(super) const USO: &str = "Uso:\n  universo relacion <origen> <clave> <destino> [--quitar] [--autor <a>] [--motivo <m>] [--dry-run] [--format text|json]\n  universo atributo <nota> <clave> <valor> [--quitar] [--autor <a>] [--motivo <m>] [--dry-run] [--format text|json]\n  universo nota editar <nota> [--autor <a>] [--motivo <m>] [--dry-run] [--format text|json]\n`nota editar` abre la nota en $VISUAL o $EDITOR y aplica lo que cambie.\n";

enum Accion {
    Relacion {
        origen: String,
        clave: String,
        destino: String,
    },
    Atributo {
        nota: String,
        clave: String,
        valor: String,
    },
    Editar {
        nota: String,
    },
}

pub(super) struct Solicitud {
    accion: Accion,
    raiz: PathBuf,
    manifiesto: PathBuf,
    invariantes: PathBuf,
    quitar: bool,
    autor: Option<String>,
    motivo: Option<String>,
    dry_run: bool,
    formato: Formato,
}

impl Solicitud {
    fn comando(&self) -> &'static str {
        match self.accion {
            Accion::Relacion { .. } => "relacion",
            Accion::Atributo { .. } => "atributo",
            Accion::Editar { .. } => "nota editar",
        }
    }
}

/// `comando` es `relacion`, `atributo` o `nota`.
pub(super) fn parsear(comando: &str, args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let mut posicionales = Vec::new();
    let mut valores = std::collections::BTreeMap::new();
    let (mut quitar, mut dry_run) = (false, false);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let bandera = match arg.as_str() {
            "--quitar" if comando != "nota" => Some(&mut quitar),
            "--dry-run" => Some(&mut dry_run),
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
            "--manifiesto" => "manifiesto",
            "--invariantes" => "invariantes",
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
    let accion = match (comando, posicionales.as_slice()) {
        ("relacion", [_, origen, clave, destino]) => Accion::Relacion {
            origen: origen.clone(),
            clave: clave.clone(),
            destino: destino.clone(),
        },
        ("atributo", [_, nota, clave, valor]) => Accion::Atributo {
            nota: nota.clone(),
            clave: clave.clone(),
            valor: valor.clone(),
        },
        ("nota", [_, sub, nota]) if sub == "editar" => Accion::Editar { nota: nota.clone() },
        _ => return Err(fail()),
    };
    let formato = match valores.remove("format").as_deref() {
        None | Some("text") => Formato::Texto,
        Some("json") => Formato::Json,
        Some(_) => return Err(fail()),
    };
    Ok(Solicitud {
        accion,
        raiz: PathBuf::from(&posicionales[0]),
        manifiesto: PathBuf::from(valores.remove("manifiesto").ok_or_else(fail)?),
        invariantes: PathBuf::from(valores.remove("invariantes").ok_or_else(fail)?),
        quitar,
        autor: valores.remove("autor"),
        motivo: valores.remove("motivo"),
        dry_run,
        formato,
    })
}

fn existe(e: &Estructura, ruta: &str) -> Result<String, Fallo> {
    let path = BundlePath::nuevo(ruta).map_err(|_| fallo("note_not_found"))?;
    e.por_path(&path).ok_or_else(|| fallo("note_not_found"))?;
    Ok(path.to_string())
}

/// Delta de una relación: agregar (si no existe) o quitar la de mayor
/// ordinal, con su etiqueta. `None`: nada que cambiar.
fn delta_relacion(
    e: &Estructura,
    origen: &str,
    clave: &str,
    destino: &str,
    quitar: bool,
) -> Result<Option<Value>, Fallo> {
    if !e.manifiesto().aristas.iter().any(|a| a.clave == clave) {
        return Err(fallo("relation_unknown"));
    }
    let (origen, destino) = (existe(e, origen)?, existe(e, destino)?);
    let atomos = e.atomos();
    let existentes: Vec<&Atomo> = atomos
        .iter()
        .filter(|a| es_arista(a, &origen, &destino, clave))
        .collect();
    if !quitar {
        if !existentes.is_empty() {
            return Ok(None);
        }
        let agregar = arista(e, &origen, &destino, clave);
        return Ok(Some(
            json!({"schema":"felix-delta-v1","add":agregar,"remove":[]}),
        ));
    }
    // Se quita la de mayor ordinal: así los ordinales que quedan no cambian.
    let Some(Atomo::Edge { id, .. }) = existentes.last() else {
        return Err(fallo("relation_not_found"));
    };
    let mut retirar = vec![existentes.last().map(|a| a.to_string()).unwrap_or_default()];
    retirar.extend(atomos.iter().filter_map(|a| match a {
        Atomo::HasLab {
            sujeto: Sujeto::Arista(otro),
            ..
        } if otro == id => Some(a.to_string()),
        _ => None,
    }));
    Ok(Some(
        json!({"schema":"felix-delta-v1","add":[],"remove":retirar}),
    ))
}

/// Delta de un atributo. Uno de un solo valor se reemplaza; uno con varios
/// valores suma o quita ese valor.
fn delta_atributo(
    e: &Estructura,
    nota: &str,
    clave: &str,
    valor: &str,
    quitar: bool,
) -> Result<Option<Value>, Fallo> {
    let nota = existe(e, nota)?;
    let predicado = e
        .manifiesto()
        .predicados
        .iter()
        .find(|p| p.clave == clave || p.nombre == clave)
        .ok_or_else(|| fallo("attribute_unknown"))?;
    let valido = if predicado.fecha {
        universo_core::manifiesto::es_fecha_iso(valor)
    } else {
        predicado.dominio.contains(valor)
    };
    if !valido {
        return Err(fallo("value_not_in_domain"));
    }
    let actuales: Vec<String> = e
        .atomos()
        .into_iter()
        .filter_map(|a| match a {
            Atomo::Ext {
                predicado: p,
                sujeto,
                constante,
            } if p == predicado.nombre && sujeto.como_str() == nota => Some(constante),
            _ => None,
        })
        .collect();
    let atomo = |v: &str| format!("{}({nota}, {v})", predicado.nombre);
    let presente = actuales.iter().any(|v| v == valor);
    let (agregar, retirar): (Vec<String>, Vec<String>) = if quitar {
        if !presente {
            return Err(fallo("attribute_value_not_found"));
        }
        (vec![], vec![atomo(valor)])
    } else if presente && (predicado.multivalor || actuales.len() == 1) {
        return Ok(None);
    } else if predicado.multivalor {
        (vec![atomo(valor)], vec![])
    } else {
        (
            vec![atomo(valor)],
            actuales.iter().map(|v| atomo(v)).collect(),
        )
    };
    Ok(Some(
        json!({"schema":"felix-delta-v1","add":agregar,"remove":retirar}),
    ))
}

/// Directorio temporal propio, que se borra al terminar.
struct DirTemporal(PathBuf);

impl DirTemporal {
    fn crear() -> std::io::Result<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let ruta =
            std::env::temp_dir().join(format!("felix-borrador-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&ruta)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self(ruta))
    }
}

impl Drop for DirTemporal {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Archivos regulares bajo `dir`, por ruta relativa con `/`.
fn archivos(dir: &Path) -> std::io::Result<std::collections::BTreeMap<String, Vec<u8>>> {
    fn recorrer(
        base: &Path,
        dir: &Path,
        salida: &mut std::collections::BTreeMap<String, Vec<u8>>,
    ) -> std::io::Result<()> {
        for entrada in std::fs::read_dir(dir)? {
            let entrada = entrada?;
            let tipo = entrada.file_type()?;
            if tipo.is_dir() {
                recorrer(base, &entrada.path(), salida)?;
            } else if tipo.is_file() {
                let relativa = entrada
                    .path()
                    .strip_prefix(base)
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                salida.insert(relativa, std::fs::read(entrada.path())?);
            }
        }
        Ok(())
    }
    let mut salida = std::collections::BTreeMap::new();
    recorrer(dir, dir, &mut salida)?;
    Ok(salida)
}

fn copiar(origen: &Path, destino: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destino)?;
    for entrada in std::fs::read_dir(origen)? {
        let entrada = entrada?;
        let tipo = entrada.file_type()?;
        let hacia = destino.join(entrada.file_name());
        if tipo.is_dir() {
            copiar(&entrada.path(), &hacia)?;
        } else if tipo.is_file() {
            std::fs::copy(entrada.path(), hacia)?;
        }
    }
    Ok(())
}

/// Abre `archivo` en `$VISUAL` o `$EDITOR` y espera a que termine.
fn editar(archivo: &Path) -> Result<(), Fallo> {
    let editor = ["VISUAL", "EDITOR"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
        .ok_or_else(|| fallo("editor_missing"))?;
    #[cfg(unix)]
    let estado = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(archivo)
        .status();
    // cmd.exe no entiende las comillas escapadas que agrega `arg`: la línea
    // va tal cual, como la escribiría el usuario.
    #[cfg(windows)]
    let estado = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .raw_arg(format!("/C {editor} \"{}\"", archivo.display()))
            .status()
    };
    match estado {
        Ok(estado) if estado.success() => Ok(()),
        _ => Err(fallo("editor_failed")),
    }
}

/// Delta de editar una nota: copia el bundle a un borrador, abre la nota en el
/// editor y compara (`universo delta --desde`).
fn delta_edicion(s: &Solicitud, e: &Estructura, nota: &str) -> Result<Option<Value>, Fallo> {
    let ruta = existe(e, nota)?;
    let borrador = DirTemporal::crear().map_err(|_| fallo("edit_io"))?;
    copiar(&s.raiz, &borrador.0).map_err(|_| fallo("edit_io"))?;
    let archivo = format!("{ruta}.md");
    editar(&borrador.0.join(&archivo))?;
    // Sólo cuenta lo que cambió en la nota abierta: un editor que toca otras
    // notas o archivos no debe colar esos cambios bajo este motivo.
    let (antes, despues) = (
        archivos(&s.raiz).map_err(|_| fallo("edit_io"))?,
        archivos(&borrador.0).map_err(|_| fallo("edit_io"))?,
    );
    let ajenos: std::collections::BTreeSet<String> = antes
        .keys()
        .chain(despues.keys())
        .filter(|k| **k != archivo && antes.get(*k) != despues.get(*k))
        .map(|k| k.trim_end_matches(".md").to_string())
        .collect();
    if !ajenos.is_empty() {
        return Err(Fallo {
            codigo: "edit_outside_note",
            detalle: json!(ajenos),
        });
    }
    let manifiesto = cargar_manifiesto(&s.manifiesto).map_err(|_| fallo("manifest_invalid"))?;
    let delta =
        delta_desde_borrador(&s.raiz, &borrador.0, manifiesto).map_err(|e| fallo(e.codigo()))?;
    if delta.agregar().is_empty()
        && delta.retirar().is_empty()
        && delta.crear().is_empty()
        && delta.cuerpos().is_empty()
    {
        return Ok(None);
    }
    let texto = serializar_delta_json_canonico(&delta).map_err(|_| fallo("delta_invalid"))?;
    serde_json::from_str(&texto)
        .map(Some)
        .map_err(|_| fallo("delta_invalid"))
}

enum Resultado {
    SinCambios,
    Hecho {
        aplicado: bool,
        sellado: bool,
        registrado: bool,
        descripcion: String,
    },
    Rechazado {
        reglas: Vec<Value>,
        conflictos: usize,
    },
}

fn correr(s: &Solicitud) -> Result<Resultado, Fallo> {
    let vista = VistaSimulacion::capturar(&s.raiz, &s.manifiesto, &s.invariantes)
        .map_err(|e| fallo(e.codigo()))?;
    let e = vista.estructura();
    let (delta, descripcion) = match &s.accion {
        Accion::Relacion {
            origen,
            clave,
            destino,
        } => (
            delta_relacion(e, origen, clave, destino, s.quitar)?,
            format!("{origen} -{clave}-> {destino}"),
        ),
        Accion::Atributo { nota, clave, valor } => (
            delta_atributo(e, nota, clave, valor, s.quitar)?,
            format!("{nota} {clave}={valor}"),
        ),
        Accion::Editar { nota } => (delta_edicion(s, e, nota)?, nota.clone()),
    };
    let Some(delta) = delta else {
        return Ok(Resultado::SinCambios);
    };
    let accion = if s.quitar { "quitar" } else { s.comando() };
    let procedencia = Procedencia {
        autor: s.autor.clone(),
        motivo: Some(s.motivo.clone().unwrap_or_else(|| {
            format!("{accion}: {descripcion}")
                .chars()
                .take(500)
                .collect()
        })),
    };
    let rutas = Rutas {
        raiz: &s.raiz,
        manifiesto: &s.manifiesto,
        invariantes: &s.invariantes,
    };
    Ok(
        match simular_y_aplicar(&vista, &rutas, &delta, &procedencia, s.dry_run)? {
            Veredicto::Admitido {
                aplicado,
                sellado,
                registrado,
            } => Resultado::Hecho {
                aplicado,
                sellado,
                registrado,
                descripcion,
            },
            Veredicto::Rechazado { reglas, conflictos } => {
                Resultado::Rechazado { reglas, conflictos }
            }
        },
    )
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let comando = s.comando();
    let (payload, codigo) = match (correr(&s), s.formato) {
        (Ok(Resultado::SinCambios), Formato::Json) => (
            format!(
                "{}\n",
                json!({"schema_version":1,"command":comando,"status":"no_changes","errors":[]})
            ),
            0,
        ),
        (Ok(Resultado::SinCambios), Formato::Texto) => (format!("{comando}: no changes\n"), 0),
        (
            Ok(Resultado::Hecho {
                aplicado,
                sellado,
                registrado,
                descripcion,
            }),
            Formato::Json,
        ) => (
            format!(
                "{}\n",
                json!({"schema_version":1,"command":comando,
                       "status": if aplicado { "applied" } else { "admit" },
                       "target": descripcion, "dry_run": s.dry_run,
                       "sealed": sellado, "logged": registrado, "errors":[]})
            ),
            0,
        ),
        (
            Ok(Resultado::Hecho {
                aplicado,
                sellado,
                registrado,
                descripcion,
            }),
            Formato::Texto,
        ) => {
            let mut texto = if aplicado {
                let sello = if sellado { "yes" } else { "no" };
                format!("{comando}: applied\ntarget: {descripcion}\nsealed: {sello}\n")
            } else {
                format!("{comando}: admit (dry run, nothing written)\ntarget: {descripcion}\n")
            };
            if aplicado && !registrado {
                texto.push_str(
                    "history: not recorded (el cambio se aplicó, pero falta su entrada en el log)\n",
                );
            }
            (visible(&texto), 0)
        }
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
            (visible(&texto), 1)
        }
    };
    if !escribir_salida(salida, error, payload.as_bytes(), "stdout") {
        return 1;
    }
    codigo
}
