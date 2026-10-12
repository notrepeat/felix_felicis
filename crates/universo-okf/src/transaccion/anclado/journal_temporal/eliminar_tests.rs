use super::*;
use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, RawTreeDigest};
use crate::transaccion::lock::LockEscritura;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn crear_root(temp: &Path) -> PathBuf {
    let root = temp.join("bundle");
    fs::create_dir(&root).unwrap();
    root
}

fn nombre_journal(root: &Path) -> PathBuf {
    crate::transaccion::raiz::UbicacionRaiz::nueva(root)
        .unwrap()
        .journal()
        .as_path()
        .to_path_buf()
}

fn identidad_prueba(id: u64) -> DirectoryIdentity {
    #[cfg(unix)]
    {
        DirectoryIdentity::unix(7, id)
    }
    #[cfg(windows)]
    {
        DirectoryIdentity::windows(7, format!("{id:032x}"))
    }
}

fn fingerprint_prueba(id: u64, byte: char) -> ArtifactFingerprint {
    ArtifactFingerprint::new(
        identidad_prueba(id),
        RawTreeDigest::from_hex(byte.to_string().repeat(64)).unwrap(),
    )
}

fn journal_prueba(parent: DirectoryIdentity) -> Journal {
    Journal::new(
        "0123456789abcdef0123456789abcdef",
        parent,
        fingerprint_prueba(2, 'a'),
        fingerprint_prueba(3, 'b'),
    )
    .unwrap()
}

fn identidad_de_ruta(path: &Path) -> IdentidadArchivoJournal {
    identidad_archivo(&File::open(path).unwrap()).unwrap()
}

#[test]
fn elimina_coincidencia_y_confirma_ausencia_idempotente() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let expected = journal_prueba(capability.identidad_parent.journal_identity().unwrap());
    let canonical = nombre_journal(&root);
    fs::write(&canonical, expected.encode().unwrap()).unwrap();
    let identity = capability.abrir_y_leer_canonico().unwrap().unwrap().0;

    assert_eq!(
        capability.eliminar_canonico_si_coincide(identity, &expected),
        ResultadoEliminacionJournal::Eliminado
    );
    assert!(!canonical.exists());
    assert_eq!(
        capability.eliminar_canonico_si_coincide(identity, &expected),
        ResultadoEliminacionJournal::Ausente
    );
}

#[test]
fn journal_canonico_ausente_es_resultado_idempotente() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let expected = journal_prueba(capability.identidad_parent.journal_identity().unwrap());
    let decoy = temp.path().join("identity-canary");
    fs::write(&decoy, b"preserved").unwrap();

    assert_eq!(
        capability.eliminar_canonico_si_coincide(identidad_de_ruta(&decoy), &expected),
        ResultadoEliminacionJournal::Ausente
    );
    assert_eq!(fs::read(decoy).unwrap(), b"preserved");
}

#[test]
fn preserva_reemplazo_con_identidad_distinta() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let expected = journal_prueba(capability.identidad_parent.journal_identity().unwrap());
    let canonical = nombre_journal(&root);
    let replacement = temp.path().join("replacement");
    let bytes = expected.encode().unwrap();
    fs::write(&canonical, &bytes).unwrap();
    let identity = capability.abrir_y_leer_canonico().unwrap().unwrap().0;
    fs::write(&replacement, &bytes).unwrap();
    assert_ne!(identity, identidad_de_ruta(&replacement));
    fs::remove_file(&canonical).unwrap();
    fs::rename(&replacement, &canonical).unwrap();

    assert_eq!(
        capability.eliminar_canonico_si_coincide(identity, &expected),
        ResultadoEliminacionJournal::Conflicto(ErrorJournalAnclado::IdentityMismatch)
    );
    assert_eq!(fs::read(canonical).unwrap(), bytes);
}

#[test]
fn preserva_registro_canonico_distinto_con_la_misma_identidad() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let expected = journal_prueba(capability.identidad_parent.journal_identity().unwrap());
    let canonical = nombre_journal(&root);
    fs::write(&canonical, expected.encode().unwrap()).unwrap();
    let identity = capability.abrir_y_leer_canonico().unwrap().unwrap().0;
    let mut different = expected.clone();
    different
        .advance(crate::transaccion::journal::JournalState::BackupReady)
        .unwrap();
    let bytes = different.encode().unwrap();
    fs::write(&canonical, &bytes).unwrap();
    assert_eq!(identity, identidad_de_ruta(&canonical));

    assert_eq!(
        capability.eliminar_canonico_si_coincide(identity, &expected),
        ResultadoEliminacionJournal::Conflicto(ErrorJournalAnclado::JournalMismatch)
    );
    assert_eq!(fs::read(canonical).unwrap(), bytes);
}

#[test]
fn preserva_journal_malformado() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let expected = journal_prueba(capability.identidad_parent.journal_identity().unwrap());
    let canonical = nombre_journal(&root);
    let bytes = b"malformed-canonical-canary";
    fs::write(&canonical, bytes).unwrap();

    assert_eq!(
        capability.eliminar_canonico_si_coincide(identidad_de_ruta(&canonical), &expected),
        ResultadoEliminacionJournal::Conflicto(ErrorJournalAnclado::InvalidJournal(
            JournalError::Malformed
        ))
    );
    assert_eq!(fs::read(canonical).unwrap(), bytes);
}

#[test]
fn preserva_directorio_canonico_y_canary() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let expected = journal_prueba(capability.identidad_parent.journal_identity().unwrap());
    let canonical = nombre_journal(&root);
    fs::create_dir(&canonical).unwrap();
    fs::write(canonical.join("canary"), b"directory-canary").unwrap();
    let identity = identidad_de_ruta(&canonical.join("canary"));

    assert_eq!(
        capability.eliminar_canonico_si_coincide(identity, &expected),
        ResultadoEliminacionJournal::Conflicto(ErrorJournalAnclado::RegularFileRequired)
    );
    assert_eq!(
        fs::read(canonical.join("canary")).unwrap(),
        b"directory-canary"
    );
}

#[cfg(unix)]
#[test]
fn preserva_symlink_canonico_y_canary_externo() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let expected = journal_prueba(capability.identidad_parent.journal_identity().unwrap());
    let canonical = nombre_journal(&root);
    let canary = temp.path().join("outside-canary");
    fs::write(&canary, b"outside-canary").unwrap();
    symlink(&canary, &canonical).unwrap();

    assert_eq!(
        capability.eliminar_canonico_si_coincide(identidad_de_ruta(&canary), &expected),
        ResultadoEliminacionJournal::Conflicto(ErrorJournalAnclado::ReparseNotAllowed)
    );
    assert!(
        fs::symlink_metadata(canonical)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(canary).unwrap(), b"outside-canary");
}

#[cfg(windows)]
#[test]
fn preserva_junction_canonica_y_canary_externo() {
    let temp = tempdir().unwrap();
    if crate::transaccion::anclado::renombrar_test_support::saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let expected = journal_prueba(capability.identidad_parent.journal_identity().unwrap());
    let canonical = nombre_journal(&root);
    let target = temp.path().join("outside-canary");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("canary"), b"outside-canary").unwrap();
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&canonical)
        .arg(&target)
        .output()
        .expect("cmd crea junction");
    if !output.status.success() {
        let detalle = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if detalle.to_ascii_lowercase().contains("privilege") {
            eprintln!("SKIP explícito: Windows no concedió privilegio para la junction");
            return;
        }
        panic!("no se pudo crear junction: {detalle}");
    }

    assert!(matches!(
        capability
            .eliminar_canonico_si_coincide(identidad_de_ruta(&target.join("canary")), &expected),
        ResultadoEliminacionJournal::Conflicto(
            ErrorJournalAnclado::ReparseNotAllowed | ErrorJournalAnclado::RegularFileRequired
        )
    ));
    assert_eq!(fs::read(target.join("canary")).unwrap(), b"outside-canary");
}
