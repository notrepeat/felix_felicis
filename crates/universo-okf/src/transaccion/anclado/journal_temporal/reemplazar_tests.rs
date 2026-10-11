use super::*;
use crate::transaccion::journal::{
    ArtifactFingerprint, DirectoryIdentity, JournalState, RawTreeDigest,
};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use std::cell::Cell;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

struct Fixture {
    old_id: IdentidadArchivoJournal,
    temp_id: IdentidadArchivoJournal,
    old: Journal,
    candidate: Journal,
    canonical: PathBuf,
    temporary: PathBuf,
}

fn root(parent: &Path) -> PathBuf {
    let root = parent.join("bundle");
    fs::create_dir(&root).unwrap();
    root
}

fn canonical(root: &Path) -> PathBuf {
    UbicacionRaiz::nueva(root)
        .unwrap()
        .journal()
        .as_path()
        .to_path_buf()
}

fn temporary(parent: &Path) -> PathBuf {
    parent.join(".bundle.felix.journal.tmp")
}

fn file_id(path: &Path) -> IdentidadArchivoJournal {
    identidad_archivo(&File::open(path).unwrap()).unwrap()
}

fn fingerprint(id: u64, hex: char) -> ArtifactFingerprint {
    #[cfg(unix)]
    let identity = DirectoryIdentity::unix(7, id);
    #[cfg(windows)]
    let identity = DirectoryIdentity::windows(7, format!("{id:032x}"));
    ArtifactFingerprint::new(
        identity,
        RawTreeDigest::from_hex(hex.to_string().repeat(64)).unwrap(),
    )
}

fn setup(capability: &JournalTemporalAnclado<'_>, root: &Path, parent: &Path) -> Fixture {
    let parent_id = capability.identidad_parent.journal_identity().unwrap();
    let old = Journal::new(
        "0123456789abcdef0123456789abcdef",
        parent_id,
        fingerprint(2, 'a'),
        fingerprint(3, 'b'),
    )
    .unwrap();
    let mut candidate = old.clone();
    candidate.advance(JournalState::BackupReady).unwrap();
    let canonical = canonical(root);
    fs::write(&canonical, old.encode().unwrap()).unwrap();
    let old_id = file_id(&canonical);
    let temporary = temporary(parent);
    let temp_id = capability
        .crear_nuevo(&candidate.encode().unwrap())
        .unwrap();
    Fixture {
        old_id,
        temp_id,
        old,
        candidate,
        canonical,
        temporary,
    }
}

#[test]
fn reemplazo_atomico_actualiza_bytes_identidad_y_quita_temporal() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    let bytes = fixture.candidate.encode().unwrap();

    assert_eq!(
        capability.reemplazar_siguiente(fixture.old_id, fixture.temp_id, &fixture.candidate),
        ResultadoPublicacionJournal::Confirmado {
            limpieza: LimpiezaConfirmadaJournal::Completa
        }
    );
    assert_eq!(file_id(&fixture.canonical), fixture.temp_id);
    assert_eq!(fs::read(&fixture.canonical).unwrap(), bytes);
    assert!(!fixture.temporary.exists());
}

#[test]
fn solo_acepta_el_sucesor_exacto_y_no_invoca_syscall_si_no_coincide() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let mut fixture = setup(&capability, &root, temp.path());
    fs::remove_file(&fixture.temporary).unwrap();
    let parent = capability.identidad_parent.journal_identity().unwrap();
    let mut different = Journal::new(
        "fedcba9876543210fedcba9876543210",
        parent,
        fingerprint(2, 'a'),
        fingerprint(3, 'b'),
    )
    .unwrap();
    different.advance(JournalState::BackupReady).unwrap();
    fixture.temp_id = capability
        .crear_nuevo(&different.encode().unwrap())
        .unwrap();
    let bytes = different.encode().unwrap();
    let mut called = false;

    assert_eq!(
        capability.reemplazar_siguiente_con(fixture.old_id, fixture.temp_id, &different, |_, _| {
            called = true;
            Ok(())
        }),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::InvalidJournal(
            JournalError::InvalidTransition
        ))
    );
    assert!(!called);
    assert_eq!(
        fs::read(&fixture.canonical).unwrap(),
        fixture.old.encode().unwrap()
    );
    assert_eq!(fs::read(&fixture.temporary).unwrap(), bytes);
}

#[test]
fn identidades_anterior_o_temporal_distintas_no_invocan_syscall() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    let called = Cell::new(false);
    let replace = |_: &JournalTemporalAnclado<'_>, _: &File| {
        called.set(true);
        Err(ErrorJournalAnclado::Io)
    };

    assert_eq!(
        capability.reemplazar_siguiente_con(
            fixture.temp_id,
            fixture.temp_id,
            &fixture.candidate,
            replace
        ),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::IdentityMismatch)
    );
    assert!(!called.get());
    assert_eq!(
        capability.reemplazar_siguiente_con(
            fixture.old_id,
            fixture.old_id,
            &fixture.candidate,
            replace
        ),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::IdentityMismatch)
    );
    assert!(!called.get());
    assert_eq!(file_id(&fixture.canonical), fixture.old_id);
    assert_eq!(file_id(&fixture.temporary), fixture.temp_id);
}

#[test]
fn directorio_canonico_en_conflicto_preserva_canaries_y_no_invoca_syscall() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    fs::remove_file(&fixture.canonical).unwrap();
    fs::create_dir(&fixture.canonical).unwrap();
    fs::write(fixture.canonical.join("canary"), b"directory canary").unwrap();
    let mut called = false;

    assert_eq!(
        capability.reemplazar_siguiente_con(
            fixture.old_id,
            fixture.temp_id,
            &fixture.candidate,
            |_, _| {
                called = true;
                Ok(())
            }
        ),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::RegularFileRequired)
    );
    assert!(!called);
    assert_eq!(
        fs::read(fixture.canonical.join("canary")).unwrap(),
        b"directory canary"
    );
    assert_eq!(file_id(&fixture.temporary), fixture.temp_id);
}

#[path = "reemplazar_outcome_tests.rs"]
mod outcome_tests;

#[path = "reemplazar_platform_tests.rs"]
mod platform_tests;
