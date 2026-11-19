//! Cambiar las reglas o el manifiesto de un universo sin romper el historial
//! (fase 10, U5).
//!
//! Bajo el lock de escritor exige que el estado actual sea el sellado (una
//! edición externa se acepta antes con `seal`, que la deja en el historial),
//! comprueba que el bundle cumple la configuración nueva (si no, rechaza con
//! las reglas que fallan y no escribe nada), instala los archivos de forma
//! atómica, vuelve a sellar y deja una entrada `kind: "config"` en el
//! historial con los tokens del estado de partida y del nuevo y el efecto
//! sobre los átomos (el manifiesto puede cambiar qué átomos salen de las
//! mismas notas). Así `universo diff` cruza el cambio sin ver un hueco.
use super::{
    durabilidad::{self, Punto},
    estado,
    historial::{Efecto, Procedencia, Registro, anotar},
    lock::LockEscritura,
    raiz::UbicacionRaiz,
    sello::{escribir_sello, leer_sello, sincronizar_parent},
    simulacion::VistaSimulacion,
};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use universo_core::{Delta, ResultadoPropuesta};

/// Qué cambiar. `None` deja ese archivo como está.
pub struct CambioConfig<'a> {
    pub manifiesto: Option<&'a str>,
    pub invariantes: Option<&'a str>,
}

/// Resultado de un cambio de configuración.
#[derive(Debug)]
pub enum ResultadoConfig {
    /// Admitido; `aplicado` es falso con `dry_run`. Con `aplicado` y sin
    /// `sellado`, la configuración quedó instalada pero no se pudo recapturar
    /// o sellar: `universo guard` lo informa.
    Admitido {
        aplicado: bool,
        sellado: bool,
        registrado: bool,
    },
    /// El bundle actual no cumple la configuración nueva: id y mensaje de
    /// cada regla que falla (`None` si la regla no los declara).
    Rechazado {
        reglas: Vec<(Option<String>, Option<String>)>,
        conflictos: usize,
    },
}

/// Código del error, como los demás de la transacción.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorConfig(pub &'static str);

/// Directorio temporal junto a la configuración (mismo sistema de archivos,
/// modo 0700); se borra al soltarlo, también ante un panic.
struct DirTemporal(PathBuf);

impl DirTemporal {
    fn crear(junto_a: &Path) -> io::Result<Self> {
        let parent = junto_a
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut azar = [0u8; 8];
        getrandom::fill(&mut azar).map_err(|_| io::Error::other("random"))?;
        let sufijo: String = azar.iter().map(|b| format!("{b:02x}")).collect();
        let ruta = parent.join(format!(".felix-config-{sufijo}"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new().mode(0o700).create(&ruta)?;
        }
        #[cfg(not(unix))]
        fs::DirBuilder::new().create(&ruta)?;
        Ok(Self(ruta))
    }
}

impl Drop for DirTemporal {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Escribe `contenido` en `ruta` de forma atómica: temporal en el mismo
/// directorio, fsync y rename. Si falla, no deja el temporal y el archivo
/// sigue como estaba. Tras el rename, el fsync del directorio es de mejor
/// esfuerzo: el archivo ya cambió, y devolver error haría creer que no.
fn escribir_atomico(ruta: &Path, contenido: &str) -> io::Result<()> {
    let mut temporal = ruta.as_os_str().to_owned();
    temporal.push(".felix-config.tmp");
    let temporal = PathBuf::from(temporal);
    let _ = fs::remove_file(&temporal);
    let resultado = (|| {
        let mut archivo = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporal)?;
        archivo.write_all(contenido.as_bytes())?;
        durabilidad::sincronizar_archivo(&archivo, Punto::Sello)?;
        drop(archivo);
        fs::rename(&temporal, ruta)
    })();
    if resultado.is_err() {
        let _ = fs::remove_file(&temporal);
        return resultado;
    }
    let _ = match ruta.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => sincronizar_parent(parent),
        None => sincronizar_parent(Path::new(".")),
    };
    Ok(())
}

/// Un archivo de configuración que se reemplaza debe ser un archivo regular:
/// el rename cambiaría el enlace, no su destino.
fn exigir_regular(ruta: &Path) -> Result<(), ErrorConfig> {
    match fs::symlink_metadata(ruta) {
        Ok(m) if m.file_type().is_file() => Ok(()),
        Ok(m) if m.file_type().is_symlink() => Err(ErrorConfig("config_path_symlink")),
        _ => Err(ErrorConfig("config_path_invalid")),
    }
}

/// Instala los archivos; si el segundo falla, restaura el primero.
fn instalar(
    manifiesto: &Path,
    invariantes: &Path,
    cambio: &CambioConfig<'_>,
) -> Result<(), ErrorConfig> {
    let io = |_| ErrorConfig("config_io");
    let previo_manifiesto = match cambio.manifiesto {
        Some(texto) => {
            let previo = fs::read_to_string(manifiesto).map_err(io)?;
            escribir_atomico(manifiesto, texto).map_err(io)?;
            Some(previo)
        }
        None => None,
    };
    if let Some(texto) = cambio.invariantes {
        if escribir_atomico(invariantes, texto).is_err() {
            if let Some(previo) = previo_manifiesto {
                // Si también falla la restauración, el estado no es el
                // sellado y `guard` lo informa.
                let _ = escribir_atomico(manifiesto, &previo);
            }
            return Err(ErrorConfig("config_io"));
        }
    }
    Ok(())
}

/// Aplica `cambio` al universo de `raiz` (con su `manifiesto` e `invariantes`).
///
/// Errores: `provenance_invalid`; `config_transaction_pending` si hay una
/// transacción a medias; `config_seal_mismatch` si el estado actual no es el
/// sellado (una edición externa, o sin sello: `universo guard` y `seal`);
/// `config_path_symlink` o `config_path_invalid` si el manifiesto o las
/// invariantes no son archivos regulares; `config_io`; y los de captura o
/// evaluación (por ejemplo, un manifiesto o unas reglas mal formados).
pub fn aplicar_config(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
    cambio: &CambioConfig<'_>,
    procedencia: &Procedencia,
    dry_run: bool,
) -> Result<ResultadoConfig, ErrorConfig> {
    procedencia.validar().map_err(|e| ErrorConfig(e.codigo()))?;
    exigir_regular(manifiesto)?;
    exigir_regular(invariantes)?;
    let ubicacion = UbicacionRaiz::nueva(raiz).map_err(|_| ErrorConfig("config_root_invalid"))?;
    let _lock = LockEscritura::adquirir_en(&ubicacion).map_err(|e| ErrorConfig(e.codigo()))?;
    if !estado::inspeccionar(raiz)
        .map_err(|e| ErrorConfig(e.codigo()))?
        .limpio()
    {
        return Err(ErrorConfig("config_transaction_pending"));
    }
    let antes = VistaSimulacion::capturar(raiz, manifiesto, invariantes)
        .map_err(|e| ErrorConfig(e.codigo()))?;
    // Sellar aquí aceptaría en silencio una edición externa.
    match leer_sello(&ubicacion) {
        Ok(Some(sellado)) if sellado == antes.snapshot_token() => {}
        _ => return Err(ErrorConfig("config_seal_mismatch")),
    }

    // La configuración nueva, en archivos temporales, para evaluarla sin tocar
    // la actual.
    let veredicto = {
        let directorio = DirTemporal::crear(manifiesto).map_err(|_| ErrorConfig("config_io"))?;
        let m_nuevo = directorio.0.join("manifest.toml");
        let i_nuevo = directorio.0.join("invariants.sx");
        let preparar = || -> io::Result<()> {
            match cambio.manifiesto {
                Some(texto) => fs::write(&m_nuevo, texto)?,
                None => {
                    fs::copy(manifiesto, &m_nuevo)?;
                }
            }
            match cambio.invariantes {
                Some(texto) => fs::write(&i_nuevo, texto)?,
                None => {
                    fs::copy(invariantes, &i_nuevo)?;
                }
            }
            Ok(())
        };
        preparar().map_err(|_| ErrorConfig("config_io"))?;
        let vista = VistaSimulacion::capturar(raiz, &m_nuevo, &i_nuevo)
            .map_err(|e| ErrorConfig(e.codigo()))?;
        let nada = Delta::nuevo(vec![], vec![]).map_err(|_| ErrorConfig("config_invalid"))?;
        match vista.proponer(&nada).map_err(|e| ErrorConfig(e.codigo()))? {
            ResultadoPropuesta::Admitida(_) => None,
            ResultadoPropuesta::RechazadaConflictos(c) => Some(ResultadoConfig::Rechazado {
                reglas: Vec::new(),
                conflictos: c.total_conflictos(),
            }),
            ResultadoPropuesta::RechazadaInvariantes(fuentes) => {
                let reglas = vista.invariantes();
                Some(ResultadoConfig::Rechazado {
                    reglas: fuentes
                        .iter()
                        .map(|p| {
                            let f = reglas.get(p.source_index);
                            (
                                f.and_then(|f| f.id.clone()),
                                f.and_then(|f| f.mensaje.clone()),
                            )
                        })
                        .collect(),
                    conflictos: 0,
                })
            }
        }
    };
    if let Some(rechazo) = veredicto {
        return Ok(rechazo);
    }
    if dry_run {
        return Ok(ResultadoConfig::Admitido {
            aplicado: false,
            sellado: false,
            registrado: false,
        });
    }

    // Instalar, recapturar, registrar y sellar. Desde aquí la configuración
    // ya cambió: un fallo posterior se informa, no se devuelve como error.
    instalar(manifiesto, invariantes, cambio)?;
    let Ok(despues) = VistaSimulacion::capturar(raiz, manifiesto, invariantes) else {
        return Ok(ResultadoConfig::Admitido {
            aplicado: true,
            sellado: false,
            registrado: false,
        });
    };
    let hasta = despues.snapshot_token();
    let efecto = Efecto::entre(antes.estructura(), despues.estructura());
    let registrado = anotar(
        &ubicacion,
        procedencia,
        Some(hasta),
        true,
        &Registro {
            delta: None,
            efecto: Some(&efecto),
            desde: Some(antes.snapshot_token()),
            hasta: Some(hasta),
            clase: Some("config"),
        },
    )
    .is_ok();
    let sellado = escribir_sello(&ubicacion, hasta).is_ok();
    Ok(ResultadoConfig::Admitido {
        aplicado: true,
        sellado,
        registrado,
    })
}
