//! Captura inmutable del bundle y de sus entradas raw de configuración.
//!
//! Esta etapa rechaza enlaces observables y parsea solo desde buffers ya
//! capturados. El recorrido actual usa APIs por ruta: detecta enlaces que
//! existen al inspeccionar y no sigue el archivo final al abrir, pero no
//! neutraliza un reemplazo concurrente de un directorio ancestro entre esas
//! operaciones. Por eso el módulo y su API siguen internos. Apply no puede
//! llamar esta captura hasta que U6 le pase un root anclado y validado; el
//! lock de U5 coordina escritores del producto, pero no elimina ese TOCTOU.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use universo_core::{Estructura, SentenciaFuente, parsear_programa};

use crate::bundle::{directorio_layout_seguro, parsear_bundle};
use crate::manifiesto_toml::parsear_manifiesto;

use super::inventario::{
    inventariar_root_con_lector, leer_archivo_regular, ruta_absoluta_lexica,
    validar_archivo_sin_enlaces,
};
use super::snapshot::{
    EntradaSnapshot, ErrorSnapshot, SnapshotToken, TipoEntradaSnapshot, calcular_token_snapshot,
};

/// Snapshot completo que conserva las entradas raw y sus vistas parseadas.
/// Sus campos no son mutables desde el exterior y su representación se redacta.
pub(crate) struct Snapshot {
    token: SnapshotToken,
    entradas: Vec<EntradaSnapshot>,
    manifiesto_raw: Arc<[u8]>,
    invariantes_raw: Arc<[u8]>,
    estructura: Estructura,
    fuentes_invariantes: Vec<SentenciaFuente>,
}

impl Snapshot {
    /// Token opaco; la representación solo se revela mediante `to_hex`.
    pub(crate) fn token(&self) -> SnapshotToken {
        self.token
    }

    /// Entradas raw del root, ordenadas por bytes UTF-8 de la ruta.
    pub(crate) fn entradas(&self) -> &[EntradaSnapshot] {
        &self.entradas
    }

    /// Bytes exactos del manifiesto capturado.
    pub(crate) fn manifiesto_raw(&self) -> &[u8] {
        &self.manifiesto_raw
    }

    /// Bytes exactos del programa de invariantes capturado.
    pub(crate) fn invariantes_raw(&self) -> &[u8] {
        &self.invariantes_raw
    }

    /// Estructura del bundle parseada desde los bytes de `entradas`.
    pub(crate) fn estructura(&self) -> &Estructura {
        &self.estructura
    }

    /// Fuentes del programa parseadas desde `invariantes_raw`.
    pub(crate) fn fuentes_invariantes(&self) -> &[SentenciaFuente] {
        &self.fuentes_invariantes
    }
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Snapshot { .. }")
    }
}

/// Lee y captura un root, su manifiesto y su programa de invariantes.
///
/// Toda entrada regular del root se lee una vez a un buffer raw; manifiesto,
/// invariantes y Markdown se parsean desde esos mismos buffers. Las rutas de
/// configuración externas al root también se leen una sola vez y se rechazan
/// cuando algún componente observado es un symlink o reparse point. No usar
/// desde apply hasta recibir un root anclado por el guard de U6.
pub(crate) fn capturar_snapshot(
    raiz: &Path,
    ruta_manifiesto: &Path,
    ruta_invariantes: &Path,
    version_motor: &str,
    policy_id: &str,
) -> Result<Snapshot, ErrorSnapshot> {
    capturar_snapshot_con_lector(
        raiz,
        ruta_manifiesto,
        ruta_invariantes,
        version_motor,
        policy_id,
        leer_archivo_regular,
    )
}

fn capturar_snapshot_con_lector<F>(
    raiz: &Path,
    ruta_manifiesto: &Path,
    ruta_invariantes: &Path,
    version_motor: &str,
    policy_id: &str,
    mut leer: F,
) -> Result<Snapshot, ErrorSnapshot>
where
    F: FnMut(&Path) -> Result<Vec<u8>, ErrorSnapshot>,
{
    let raiz = ruta_absoluta_lexica(raiz)?;
    let entradas = inventariar_root_con_lector(&raiz, &mut leer)?;
    let mut cache_externa = HashMap::new();
    let manifiesto_raw = bytes_fuente(
        &raiz,
        ruta_manifiesto,
        &entradas,
        &mut cache_externa,
        &mut leer,
    )?;
    let invariantes_raw = bytes_fuente(
        &raiz,
        ruta_invariantes,
        &entradas,
        &mut cache_externa,
        &mut leer,
    )?;

    let manifiesto_texto =
        std::str::from_utf8(&manifiesto_raw).map_err(|_| ErrorSnapshot::ManifiestoInvalido)?;
    let manifiesto =
        parsear_manifiesto(manifiesto_texto).map_err(|_| ErrorSnapshot::ManifiestoInvalido)?;
    let archivos = archivos_markdown(
        &entradas,
        &manifiesto.layout.directorios,
        &manifiesto.layout.ignorados,
    )?;
    let estructura =
        parsear_bundle(&archivos, manifiesto).map_err(|_| ErrorSnapshot::BundleInvalido)?;

    let invariantes_texto =
        std::str::from_utf8(&invariantes_raw).map_err(|_| ErrorSnapshot::InvariantesInvalidas)?;
    let fuentes_invariantes =
        parsear_programa(invariantes_texto).map_err(|_| ErrorSnapshot::InvariantesInvalidas)?;
    let token = calcular_token_snapshot(
        version_motor,
        policy_id,
        &manifiesto_raw,
        &invariantes_raw,
        &entradas,
    )?;

    Ok(Snapshot {
        token,
        entradas,
        manifiesto_raw,
        invariantes_raw,
        estructura,
        fuentes_invariantes,
    })
}

fn bytes_fuente<F>(
    raiz: &Path,
    ruta: &Path,
    entradas: &[EntradaSnapshot],
    cache_externa: &mut HashMap<PathBuf, Arc<[u8]>>,
    leer: &mut F,
) -> Result<Arc<[u8]>, ErrorSnapshot>
where
    F: FnMut(&Path) -> Result<Vec<u8>, ErrorSnapshot>,
{
    let absoluta = ruta_absoluta_lexica(ruta)?;
    if absoluta.starts_with(raiz) {
        let relativa = absoluta
            .strip_prefix(raiz)
            .map_err(|_| ErrorSnapshot::RutaNoRepresentable)?
            .to_str()
            .ok_or(ErrorSnapshot::RutaNoUtf8)?
            .replace('\\', "/");
        return entradas
            .iter()
            .find(|entrada| {
                entrada.ruta() == relativa && entrada.tipo() == TipoEntradaSnapshot::Archivo
            })
            .and_then(EntradaSnapshot::bytes)
            .map(Arc::from)
            .ok_or(ErrorSnapshot::Io);
    }

    validar_archivo_sin_enlaces(&absoluta)?;
    if let Some(bytes) = cache_externa.get(&absoluta) {
        return Ok(Arc::clone(bytes));
    }
    let bytes: Arc<[u8]> = Arc::from(leer(&absoluta)?);
    cache_externa.insert(absoluta, Arc::clone(&bytes));
    Ok(bytes)
}

fn archivos_markdown(
    entradas: &[EntradaSnapshot],
    directorios: &[String],
    ignorados: &[String],
) -> Result<Vec<(String, String)>, ErrorSnapshot> {
    let mut archivos = Vec::new();
    for directorio in directorios {
        if !directorio_layout_seguro(directorio)
            || !entradas.iter().any(|entrada| {
                entrada.ruta() == directorio && entrada.tipo() == TipoEntradaSnapshot::Directorio
            })
        {
            return Err(ErrorSnapshot::BundleInvalido);
        }
        let prefijo = format!("{directorio}/");
        for entrada in entradas {
            if entrada.tipo() != TipoEntradaSnapshot::Archivo
                || !entrada.ruta().starts_with(&prefijo)
                || Path::new(entrada.ruta())
                    .extension()
                    .and_then(|ext| ext.to_str())
                    != Some("md")
                || ignorados
                    .iter()
                    .any(|nombre| entrada.ruta().rsplit('/').next() == Some(nombre.as_str()))
            {
                continue;
            }
            let bytes = entrada.bytes().ok_or(ErrorSnapshot::BundleInvalido)?;
            let texto = std::str::from_utf8(bytes).map_err(|_| ErrorSnapshot::BundleInvalido)?;
            archivos.push((entrada.ruta().to_owned(), texto.to_owned()));
        }
    }
    Ok(archivos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::tempdir;

    const MANIFIESTO: &str = r#"version = "snapshot-test/1"

[nodos]
sortes = ["a", "Retirado", "Patrón"]

[nodos.disco]
"Patrón" = "pattern"

[layout]
directorios = ["knowledge"]
sin_iota = []
ignorados = ["ignored.md"]
"#;

    #[test]
    fn captura_reutiliza_buffers_del_root_para_configuracion_y_parsing() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        fs::write(root.path().join("manifest.toml"), MANIFIESTO).unwrap();
        fs::write(root.path().join("invariants.smt2"), "\n").unwrap();
        fs::write(
            root.path().join("knowledge/a.md"),
            "---\ntype: a\ntitle: A\n---\n",
        )
        .unwrap();
        let mut lecturas = HashMap::<PathBuf, usize>::new();
        let snapshot = capturar_snapshot_con_lector(
            root.path(),
            &root.path().join("manifest.toml"),
            &root.path().join("invariants.smt2"),
            "engine-test/1",
            "policy-test/1",
            |path| {
                *lecturas.entry(path.to_path_buf()).or_default() += 1;
                leer_archivo_regular(path)
            },
        )
        .unwrap();

        assert!(lecturas.values().all(|&count| count == 1));
        assert_eq!(lecturas.len(), 3);
        assert_eq!(snapshot.manifiesto_raw(), MANIFIESTO.as_bytes());
        assert_eq!(snapshot.invariantes_raw(), b"\n");
        assert_eq!(snapshot.estructura().nodos().count(), 1);
        assert!(snapshot.fuentes_invariantes().is_empty());
        assert!(!format!("{snapshot:?}").contains("snapshot-test"));
        assert!(!format!("{snapshot:?}").contains("knowledge/a.md"));
    }

    #[test]
    fn replay_y_cambios_raw_auxiliares_y_directorios_cambian_la_identidad() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        fs::create_dir_all(root.join("knowledge")).unwrap();
        let manifest = temp.path().join("manifest.toml");
        let invariants = temp.path().join("invariants.smt2");
        fs::write(&manifest, MANIFIESTO).unwrap();
        fs::write(&invariants, "\n").unwrap();
        fs::write(root.join("knowledge/a.md"), "---\ntype: a\ntitle: A\n---\n").unwrap();
        fs::write(root.join("knowledge/ignored.md"), [0xff, 0x00]).unwrap();
        fs::write(root.join("payload.bin"), []).unwrap();

        let capturar =
            || capturar_snapshot(&root, &manifest, &invariants, "engine/1", "policy/1").unwrap();
        let base = capturar();
        let replay = capturar();
        assert_eq!(base.token(), replay.token());
        assert_eq!(base.estructura().nodos().count(), 1);
        assert!(base.fuentes_invariantes().is_empty());
        assert!(base.entradas().iter().any(|entry| {
            entry.ruta() == "knowledge/ignored.md" && entry.bytes() == Some([0xff, 0x00].as_slice())
        }));

        fs::write(root.join("payload.bin"), [0, 255, 1]).unwrap();
        let binary_changed = capturar().token();
        assert_ne!(base.token(), binary_changed);
        fs::write(&manifest, format!("{MANIFIESTO}\n")).unwrap();
        let manifest_changed = capturar().token();
        assert_ne!(binary_changed, manifest_changed);
        fs::write(&manifest, MANIFIESTO).unwrap();
        fs::write(&invariants, "\n\n").unwrap();
        let invariants_changed = capturar().token();
        assert_ne!(binary_changed, invariants_changed);

        fs::write(&invariants, "\n").unwrap();
        fs::create_dir(root.join("empty-dir")).unwrap();
        assert_ne!(base.token(), capturar().token());
    }

    #[test]
    fn error_de_parseo_no_revela_contenido_raw() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        let manifest = root.path().join("manifest.toml");
        let invariants = root.path().join("invariants.smt2");
        fs::write(&manifest, "PRIVATE-CANARY invalid manifest").unwrap();
        fs::write(&invariants, "").unwrap();

        let error = capturar_snapshot(root.path(), &manifest, &invariants, "engine/1", "policy/1")
            .unwrap_err();
        assert_eq!(error, ErrorSnapshot::ManifiestoInvalido);
        assert!(!format!("{error:?}").contains("PRIVATE-CANARY"));
        assert!(!error.to_string().contains("PRIVATE-CANARY"));
    }
}
