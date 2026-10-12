use super::*;
use crate::transaccion::journal::recovery::ArtifactState;
use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, Journal, RawTreeDigest};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use crate::transaccion::snapshot::calcular_digest_raw_tree;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

const TXN: &str = "0123456789abcdef0123456789abcdef";

fn crear_arbol(root: &Path, bytes: &[u8]) {
    fs::create_dir_all(root.join("empty-dir")).unwrap();
    fs::write(root.join("data.bin"), bytes).unwrap();
}

fn partes_fingerprint(root: &Path) -> (ArtifactFingerprint, DirectoryIdentity, RawTreeDigest) {
    let location = UbicacionRaiz::nueva(root).unwrap();
    let parent = DirectorioAnclado::parent_solo_lectura_desde_ubicacion(&location).unwrap();
    let name = location.ruta().file_name().unwrap();
    let artifact = parent
        .abrir_artifacto_sibling_directorio_lectura(&location, name)
        .unwrap()
        .unwrap();
    let identity = artifact
        .identidad_raiz()
        .unwrap()
        .journal_identity()
        .unwrap();
    let entries = inventariar_root_anclado(artifact).unwrap();
    let digest = calcular_digest_raw_tree(&entries).unwrap();
    (
        ArtifactFingerprint::new(identity.clone(), digest.clone()),
        identity,
        digest,
    )
}

fn fingerprint(root: &Path) -> ArtifactFingerprint {
    partes_fingerprint(root).0
}

fn parent_identity(lock: &LockEscritura) -> DirectoryIdentity {
    DirectorioAnclado::parent_solo_lectura_desde_ubicacion(lock.ubicacion())
        .unwrap()
        .identidad_parent()
        .journal_identity()
        .unwrap()
}

fn journal(lock: &LockEscritura, old: ArtifactFingerprint, new: ArtifactFingerprint) -> Journal {
    Journal::new(TXN, parent_identity(lock), old, new).unwrap()
}

fn backup_path(lock: &LockEscritura) -> PathBuf {
    lock.ubicacion().backup().as_path().to_path_buf()
}

fn stage_path(lock: &LockEscritura) -> PathBuf {
    lock.ubicacion().stage(TXN).unwrap().as_path().to_path_buf()
}

#[derive(Debug, PartialEq, Eq)]
struct FsEntry {
    kind: u8,
    bytes: Option<Vec<u8>>,
}

#[derive(PartialEq, Eq)]
struct FsSnapshot {
    entries: BTreeMap<String, FsEntry>,
    artifact_identities: BTreeMap<String, Option<DirectoryIdentity>>,
}

impl std::fmt::Debug for FsSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FsSnapshot([redacted])")
    }
}

fn filesystem_snapshot(lock: &LockEscritura) -> FsSnapshot {
    let ubicacion = lock.ubicacion();
    let backup = ubicacion.backup();
    let stage = ubicacion.stage(TXN).unwrap();
    let artifacts = [
        ("root", lock.raiz_canonica().to_path_buf()),
        ("backup", backup.as_path().to_path_buf()),
        ("stage", stage.as_path().to_path_buf()),
    ];
    let mut entries = BTreeMap::new();
    let mut pending = Vec::new();
    for (label, path) in artifacts {
        match fs::symlink_metadata(&path) {
            Ok(_) => pending.push((label.to_owned(), path)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("no se pudo inspeccionar el artifact snapshot: {error}"),
        }
    }
    while let Some((label, path)) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).unwrap();
        let (kind, bytes) = if metadata.file_type().is_symlink() {
            (b'L', None)
        } else if metadata.is_dir() {
            for entry in fs::read_dir(&path).unwrap() {
                let entry = entry.unwrap();
                pending.push((
                    format!("{label}/{}", entry.file_name().to_string_lossy()),
                    entry.path(),
                ));
            }
            (b'D', None)
        } else if metadata.is_file() {
            (b'F', Some(fs::read(&path).unwrap()))
        } else {
            (b'O', None)
        };
        entries.insert(label, FsEntry { kind, bytes });
    }

    let parent = DirectorioAnclado::parent_solo_lectura_desde_ubicacion(ubicacion).unwrap();
    let root_name = ubicacion.ruta().file_name().unwrap();
    let backup_name = ubicacion.backup().as_path().file_name().unwrap().to_owned();
    let stage_name = ubicacion
        .stage(TXN)
        .unwrap()
        .as_path()
        .file_name()
        .unwrap()
        .to_owned();
    let mut artifact_identities = BTreeMap::new();
    for (label, name) in [
        ("root", root_name),
        ("backup", backup_name.as_os_str()),
        ("stage", stage_name.as_os_str()),
    ] {
        let identity = parent
            .abrir_artifacto_sibling_directorio_lectura(ubicacion, name)
            .unwrap()
            .map(|artifact| {
                artifact
                    .identidad_raiz()
                    .unwrap()
                    .journal_identity()
                    .unwrap()
            });
        artifact_identities.insert(label.to_owned(), identity);
    }

    FsSnapshot {
        entries,
        artifact_identities,
    }
}

#[test]
fn old_absent_new_inventory_is_read_only_and_unvalidated() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    crear_arbol(&root, b"old-tree");
    let lock = LockEscritura::adquirir(&root).unwrap();
    let old = fingerprint(&root);
    let stage = stage_path(&lock);
    crear_arbol(&stage, b"new-tree");
    let new = fingerprint(&stage);
    let record = journal(&lock, old, new);
    let before = filesystem_snapshot(&lock);

    let inventory = observar_inventario_recovery(&lock, &record).unwrap();

    assert_eq!(inventory.root, ArtifactState::Old);
    assert_eq!(inventory.backup, ArtifactState::Absent);
    assert_eq!(inventory.stage, ArtifactState::New);
    assert!(!inventory.root_new_validated);
    assert_eq!(filesystem_snapshot(&lock), before);
}

#[test]
fn absent_root_old_backup_and_absent_stage_use_anchored_not_found() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    let lock = LockEscritura::adquirir(&root).unwrap();
    let backup = backup_path(&lock);
    crear_arbol(&backup, b"old-tree");
    let old = fingerprint(&backup);
    let fixture_new = temp.path().join("new-fixture");
    crear_arbol(&fixture_new, b"new-tree");
    let record = journal(&lock, old, fingerprint(&fixture_new));

    let inventory = observar_inventario_recovery(&lock, &record).unwrap();

    assert_eq!(inventory.root, ArtifactState::Absent);
    assert_eq!(inventory.backup, ArtifactState::Old);
    assert_eq!(inventory.stage, ArtifactState::Absent);
    assert!(!inventory.root_new_validated);
}

#[test]
fn digest_mismatch_with_same_directory_identity_is_unknown() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    crear_arbol(&root, b"old-tree");
    let lock = LockEscritura::adquirir(&root).unwrap();
    let old = fingerprint(&root);
    let fixture_new = temp.path().join("new-fixture");
    crear_arbol(&fixture_new, b"new-tree");
    let record = journal(&lock, old, fingerprint(&fixture_new));
    fs::write(root.join("data.bin"), b"tampered-tree").unwrap();

    let inventory = observar_inventario_recovery(&lock, &record).unwrap();

    assert_eq!(inventory.root, ArtifactState::Unknown);
    assert!(!inventory.root_new_validated);
}

#[test]
fn same_bytes_with_a_different_directory_identity_are_unknown() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    let reference = temp.path().join("reference");
    crear_arbol(&root, b"same-tree");
    crear_arbol(&reference, b"same-tree");
    let (expected, expected_identity, expected_digest) = partes_fingerprint(&reference);
    let (actual, actual_identity, actual_digest) = partes_fingerprint(&root);
    assert!(expected_identity != actual_identity);
    assert!(expected_digest == actual_digest);
    assert!(expected != actual);

    let lock = LockEscritura::adquirir(&root).unwrap();
    let fixture_new = temp.path().join("new-fixture");
    crear_arbol(&fixture_new, b"new-tree");
    let record = journal(&lock, expected, fingerprint(&fixture_new));

    let inventory = observar_inventario_recovery(&lock, &record).unwrap();

    assert_eq!(inventory.root, ArtifactState::Unknown);
}

#[test]
fn parent_identity_mismatch_fails_closed_before_observing_artifacts() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    crear_arbol(&root, b"old-tree");
    let primary_canary = root.join("parent-mismatch-canary.bin");
    fs::write(&primary_canary, b"primary-parent-canary").unwrap();
    let lock = LockEscritura::adquirir(&root).unwrap();
    let old = fingerprint(&root);
    let fixture_new = temp.path().join("new-fixture");
    crear_arbol(&fixture_new, b"new-tree");

    let other_parent = temp.path().join("other-parent");
    fs::create_dir(&other_parent).unwrap();
    let other_root = other_parent.join("bundle");
    crear_arbol(&other_root, b"other-parent-tree");
    let other_canary = other_root.join("parent-mismatch-canary.bin");
    fs::write(&other_canary, b"other-parent-canary").unwrap();
    let other_location = UbicacionRaiz::nueva(&other_root).unwrap();
    let other_identity = DirectorioAnclado::parent_solo_lectura_desde_ubicacion(&other_location)
        .unwrap()
        .identidad_parent()
        .journal_identity()
        .unwrap();
    let record = Journal::new(TXN, other_identity, old, fingerprint(&fixture_new)).unwrap();
    let before = filesystem_snapshot(&lock);
    let primary_canary_before = fs::read(&primary_canary).unwrap();
    let other_canary_before = fs::read(&other_canary).unwrap();

    let error = observar_inventario_recovery(&lock, &record).unwrap_err();

    assert_eq!(error, ErrorObservadorRecovery::ParentIdentityMismatch);
    assert_eq!(error.to_string(), "recovery_parent_identity_mismatch");
    assert_eq!(filesystem_snapshot(&lock), before);
    assert_eq!(fs::read(primary_canary).unwrap(), primary_canary_before);
    assert_eq!(fs::read(other_canary).unwrap(), other_canary_before);
}

#[cfg(unix)]
#[test]
fn sibling_fifo_root_backup_stage_no_se_bloquea() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::time::{Duration, Instant};

    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    fs::create_dir(&root).unwrap();
    let lock = LockEscritura::adquirir(&root).unwrap();
    let location = lock.ubicacion();
    let parent = DirectorioAnclado::parent_solo_lectura_desde_ubicacion(location).unwrap();
    let names = [
        location.ruta().file_name().unwrap().to_os_string(),
        location
            .backup()
            .as_path()
            .file_name()
            .unwrap()
            .to_os_string(),
        location
            .stage(TXN)
            .unwrap()
            .as_path()
            .file_name()
            .unwrap()
            .to_os_string(),
    ];

    for name in names {
        let path = temp.path().join(&name);
        let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: the nul-terminated path is live for this synchronous call.
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);

        let started = Instant::now();
        let error = parent
            .abrir_artifacto_sibling_directorio_lectura(location, &name)
            .err()
            .expect("FIFO must not open as a directory");
        assert_eq!(error, ErrorDirectorioAnclado::TipoNoAdmitido);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "opening FIFO sibling {name:?} must not block"
        );
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn regular_file_at_artifact_name_is_error_and_preserves_canary() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    crear_arbol(&root, b"old-tree");
    let lock = LockEscritura::adquirir(&root).unwrap();
    let old = fingerprint(&root);
    let fixture_new = temp.path().join("new-fixture");
    crear_arbol(&fixture_new, b"new-tree");
    let record = journal(&lock, old, fingerprint(&fixture_new));
    let canary = b"regular-file-canary";
    fs::write(backup_path(&lock), canary).unwrap();

    let error = observar_inventario_recovery(&lock, &record).unwrap_err();

    assert_eq!(error, ErrorObservadorRecovery::Io);
    assert_eq!(fs::read(backup_path(&lock)).unwrap(), canary);
    assert_eq!(fs::read(root.join("data.bin")).unwrap(), b"old-tree");
}

#[cfg(unix)]
#[test]
fn sibling_symlink_is_error_and_preserves_external_canary() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    crear_arbol(&root, b"old-tree");
    let lock = LockEscritura::adquirir(&root).unwrap();
    let old = fingerprint(&root);
    let fixture_new = temp.path().join("new-fixture");
    crear_arbol(&fixture_new, b"new-tree");
    let record = journal(&lock, old, fingerprint(&fixture_new));
    let target = temp.path().join("outside");
    crear_arbol(&target, b"external-tree");
    let canary = target.join("data.bin");
    std::os::unix::fs::symlink(&target, backup_path(&lock)).unwrap();

    let error = observar_inventario_recovery(&lock, &record).unwrap_err();

    assert_eq!(error, ErrorObservadorRecovery::Io);
    assert_eq!(fs::read(canary).unwrap(), b"external-tree");
    assert_eq!(fs::read(root.join("data.bin")).unwrap(), b"old-tree");
}

#[cfg(windows)]
#[test]
fn sibling_junction_is_error_and_preserves_external_canary() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    crear_arbol(&root, b"old-tree");
    let lock = LockEscritura::adquirir(&root).unwrap();
    let old = fingerprint(&root);
    let fixture_new = temp.path().join("new-fixture");
    crear_arbol(&fixture_new, b"new-tree");
    let record = journal(&lock, old, fingerprint(&fixture_new));
    let target = temp.path().join("outside");
    crear_arbol(&target, b"external-tree");
    let canary = target.join("data.bin");
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(backup_path(&lock))
        .arg(&target)
        .output()
        .expect("cmd crea junctions");
    assert!(
        output.status.success(),
        "no se pudo crear junction: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let error = observar_inventario_recovery(&lock, &record).unwrap_err();

    assert_eq!(error, ErrorObservadorRecovery::Io);
    assert_eq!(fs::read(canary).unwrap(), b"external-tree");
    assert_eq!(fs::read(root.join("data.bin")).unwrap(), b"old-tree");
}
