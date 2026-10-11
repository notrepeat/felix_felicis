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

#[cfg(windows)]
use std::fs;

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
    let relativa_lexica = absoluta
        .starts_with(raiz)
        .then(|| ruta_relativa(raiz, &absoluta))
        .transpose()?;
    if let Some(relativa) = relativa_lexica.as_deref()
        && let Some(bytes) = bytes_entrada(entradas, relativa)
    {
        return Ok(bytes);
    }

    #[cfg(windows)]
    {
        // `Path::starts_with` compara componentes con caso exacto incluso en
        // Windows. Resuelve la ruta solo después de rechazar reparse points,
        // y usa el spelling real devuelto por el sistema únicamente para
        // localizar el buffer ya leído; el token conserva la ruta inventariada.
        validar_archivo_sin_enlaces(&absoluta)?;
        let raiz_real = fs::canonicalize(raiz).map_err(|_| ErrorSnapshot::Io)?;
        let archivo_real = fs::canonicalize(&absoluta).map_err(|_| ErrorSnapshot::Io)?;
        if archivo_real.starts_with(&raiz_real) {
            let relativa = ruta_relativa(&raiz_real, &archivo_real)?;
            return bytes_entrada(entradas, &relativa).ok_or(ErrorSnapshot::Io);
        }
    }

    #[cfg(not(windows))]
    if relativa_lexica.is_some() {
        return Err(ErrorSnapshot::Io);
    }

    #[cfg(not(windows))]
    validar_archivo_sin_enlaces(&absoluta)?;
    if let Some(bytes) = cache_externa.get(&absoluta) {
        return Ok(Arc::clone(bytes));
    }
    let bytes: Arc<[u8]> = Arc::from(leer(&absoluta)?);
    cache_externa.insert(absoluta, Arc::clone(&bytes));
    Ok(bytes)
}

fn ruta_relativa(raiz: &Path, absoluta: &Path) -> Result<String, ErrorSnapshot> {
    absoluta
        .strip_prefix(raiz)
        .map_err(|_| ErrorSnapshot::RutaNoRepresentable)?
        .to_str()
        .ok_or(ErrorSnapshot::RutaNoUtf8)
        .map(|ruta| ruta.replace('\\', "/"))
}

fn bytes_entrada(entradas: &[EntradaSnapshot], relativa: &str) -> Option<Arc<[u8]>> {
    entradas
        .iter()
        .find(|entrada| {
            entrada.ruta() == relativa && entrada.tipo() == TipoEntradaSnapshot::Archivo
        })
        .and_then(EntradaSnapshot::bytes)
        .map(Arc::from)
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
    fn captura_detecta_altas_bajas_renombres_y_cambios_solo_de_case() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        fs::create_dir_all(root.join("knowledge")).unwrap();
        let manifest = temp.path().join("manifest.toml");
        let invariants = temp.path().join("invariants.smt2");
        fs::write(&manifest, MANIFIESTO).unwrap();
        fs::write(&invariants, "\n").unwrap();
        fs::write(root.join("knowledge/a.md"), "---\ntype: a\ntitle: A\n---\n").unwrap();

        let capturar =
            || capturar_snapshot(&root, &manifest, &invariants, "engine/1", "policy/1").unwrap();
        let base = capturar().token();

        let archivo = root.join("auxiliar.bin");
        fs::write(&archivo, []).unwrap();
        let agregado = capturar().token();
        assert_ne!(
            base, agregado,
            "agregar un archivo vacío debe cambiar el token"
        );
        fs::remove_file(&archivo).unwrap();
        assert_eq!(
            base,
            capturar().token(),
            "eliminar el archivo revierte el token"
        );

        let original = root.join("original.bin");
        let renombrado = root.join("renombrado.bin");
        fs::write(&original, [0, 255]).unwrap();
        let antes_renombrar = capturar().token();
        fs::rename(&original, &renombrado).unwrap();
        assert_ne!(antes_renombrar, capturar().token());
        fs::rename(&renombrado, &original).unwrap();
        assert_eq!(antes_renombrar, capturar().token());

        let mayusculas = root.join("Case.bin");
        let intermedio = root.join("case-temp.bin");
        let minusculas = root.join("case.bin");
        fs::write(&mayusculas, [7]).unwrap();
        let antes_case = capturar().token();
        // El paso intermedio hace que el renombre solo de case funcione también
        // en volúmenes Windows case-insensitive.
        fs::rename(&mayusculas, &intermedio).unwrap();
        fs::rename(&intermedio, &minusculas).unwrap();
        assert_ne!(antes_case, capturar().token());
    }

    #[cfg(windows)]
    #[test]
    fn alias_de_case_para_manifest_e_invariants_reusa_los_buffers_capturados() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        fs::write(root.path().join("manifest.toml"), MANIFIESTO).unwrap();
        fs::write(root.path().join("invariants.smt2"), "\n").unwrap();
        fs::write(
            root.path().join("knowledge/a.md"),
            "---\ntype: a\ntitle: A\n---\n",
        )
        .unwrap();
        // Cambia también el case de los componentes del root: con el root
        // lexical original, cambiar solo el nombre final no prueba el caso en
        // que `starts_with` clasifica la fuente como externa.
        let root_case_alias = PathBuf::from(root.path().to_string_lossy().to_ascii_uppercase());
        let mut lecturas = HashMap::<PathBuf, usize>::new();
        let snapshot = capturar_snapshot_con_lector(
            root.path(),
            &root_case_alias.join("MANIFEST.TOML"),
            &root_case_alias.join("INVARIANTS.SMT2"),
            "engine/1",
            "policy/1",
            |path| {
                let real = fs::canonicalize(path).unwrap();
                *lecturas.entry(real).or_default() += 1;
                leer_archivo_regular(path)
            },
        )
        .unwrap();

        assert_eq!(lecturas.len(), 3);
        assert!(lecturas.values().all(|&count| count == 1));
        assert_eq!(snapshot.manifiesto_raw(), MANIFIESTO.as_bytes());
        assert_eq!(snapshot.invariantes_raw(), b"\n");
    }

    #[cfg(unix)]
    #[test]
    fn rechaza_links_usados_como_ruta_de_manifest_o_invariants() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        let outside = temp.path().join("outside");
        fs::create_dir_all(root.join("knowledge")).unwrap();
        fs::create_dir(&outside).unwrap();
        let manifest_target = outside.join("manifest.toml");
        let invariants_target = outside.join("invariants.smt2");
        let manifest_link = temp.path().join("manifest-link.toml");
        let invariants_link = temp.path().join("invariants-link.smt2");
        fs::write(&manifest_target, MANIFIESTO).unwrap();
        fs::write(&invariants_target, "\n").unwrap();
        symlink(&manifest_target, &manifest_link).unwrap();
        symlink(&invariants_target, &invariants_link).unwrap();

        assert_eq!(
            capturar_snapshot(
                &root,
                &manifest_link,
                &invariants_target,
                "engine/1",
                "policy/1"
            )
            .unwrap_err(),
            ErrorSnapshot::EnlaceNoPermitido
        );
        assert_eq!(
            capturar_snapshot(
                &root,
                &manifest_target,
                &invariants_link,
                "engine/1",
                "policy/1"
            )
            .unwrap_err(),
            ErrorSnapshot::EnlaceNoPermitido
        );
    }

    #[cfg(windows)]
    #[test]
    fn rechaza_reparse_points_en_root_y_en_rutas_de_configuracion() {
        use std::os::windows::fs::{symlink_dir, symlink_file};
        use std::process::Command;

        fn junction(enlace: &Path, destino: &Path) {
            if let Err(error) = symlink_dir(destino, enlace) {
                eprintln!("aviso: symlink_dir no disponible ({error}); se usa junction");
                let resultado = Command::new("cmd")
                    .args(["/c", "mklink", "/J"])
                    .arg(enlace)
                    .arg(destino)
                    .output()
                    .expect("cmd debe poder crear junctions");
                assert!(
                    resultado.status.success(),
                    "no se pudo crear junction: {}",
                    String::from_utf8_lossy(&resultado.stderr)
                );
            }
        }

        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        let config_target = temp.path().join("config-target");
        fs::create_dir_all(root.join("knowledge")).unwrap();
        fs::create_dir(&config_target).unwrap();
        let manifest = config_target.join("manifest.toml");
        let invariants = config_target.join("invariants.smt2");
        fs::write(&manifest, MANIFIESTO).unwrap();
        fs::write(&invariants, "\n").unwrap();
        let outside_file = temp.path().join("outside.bin");
        fs::write(&outside_file, b"raw").unwrap();

        let file_link = root.join("file-link.bin");
        match symlink_file(&outside_file, &file_link) {
            Ok(()) => {
                assert_eq!(
                    capturar_snapshot(&root, &manifest, &invariants, "engine/1", "policy/1")
                        .unwrap_err(),
                    ErrorSnapshot::EnlaceNoPermitido
                );
                fs::remove_file(file_link).unwrap();
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    || error.raw_os_error() == Some(1314) =>
            {
                eprintln!(
                    "SKIP explícito: symlink_file no disponible ({error}); este subcaso no cuenta como cobertura"
                );
            }
            Err(error) => panic!("no se pudo crear symlink_file: {error}"),
        }

        let root_link = root.join("directory-link");
        junction(&root_link, &config_target);
        assert_eq!(
            capturar_snapshot(&root, &manifest, &invariants, "engine/1", "policy/1").unwrap_err(),
            ErrorSnapshot::EnlaceNoPermitido
        );
        fs::remove_dir(root_link).unwrap();

        let alias = temp.path().join("config-alias");
        junction(&alias, &config_target);
        assert_eq!(
            capturar_snapshot(
                &root,
                &alias.join("manifest.toml"),
                &invariants,
                "engine/1",
                "policy/1"
            )
            .unwrap_err(),
            ErrorSnapshot::EnlaceNoPermitido
        );
        assert_eq!(
            capturar_snapshot(
                &root,
                &manifest,
                &alias.join("invariants.smt2"),
                "engine/1",
                "policy/1"
            )
            .unwrap_err(),
            ErrorSnapshot::EnlaceNoPermitido
        );
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
