use super::*;
use crate::transaccion::journal::{
    ArtifactFingerprint, DirectoryIdentity, Journal, JournalError, JournalState, RawTreeDigest,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn crear_root(temp: &Path) -> PathBuf {
    let root = temp.join("bundle");
    fs::create_dir(&root).unwrap();
    root
}

fn nombre_temporal(temp: &Path) -> PathBuf {
    temp.join(".bundle.felix.journal.tmp")
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

fn identidad_distinta(identity: &DirectoryIdentity) -> DirectoryIdentity {
    #[cfg(unix)]
    {
        let DirectoryIdentity::Unix { device, inode } = identity else {
            unreachable!("journal identity uses the current platform");
        };
        DirectoryIdentity::unix(device ^ 1, *inode)
    }
    #[cfg(windows)]
    {
        let DirectoryIdentity::Windows { volume, file_id } = identity else {
            unreachable!("journal identity uses the current platform");
        };
        DirectoryIdentity::windows(volume ^ 1, file_id.clone())
    }
}

#[test]
fn journal_canonico_ausente_devuelve_none() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();

    assert_eq!(capability.abrir_y_leer_canonico().unwrap(), None);
    assert_eq!(capability.cargar_canonico().unwrap(), None);
}

#[test]
fn cargar_journal_prepared_canonico_lo_liga_al_parent_anclado() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let parent = capability.identidad_parent.journal_identity().unwrap();
    let expected = journal_prueba(parent.clone());
    let bytes = expected.encode().unwrap();
    fs::write(nombre_journal(&root), &bytes).unwrap();

    let (identity, actual) = capability.cargar_canonico().unwrap().unwrap();
    let (identity_raw, _) = capability.abrir_y_leer_canonico().unwrap().unwrap();

    assert_eq!(identity, identity_raw);
    assert_eq!(
        format!("{identity:?}"),
        "IdentidadArchivoJournal([redacted])"
    );
    assert_eq!(actual, expected);
    assert!(actual.parent() == &parent);
    assert_eq!(actual.state(), JournalState::Prepared);
    assert_eq!(format!("{actual:?}"), "Journal([redacted])");
}

#[test]
fn cargar_journal_rechaza_malformado_y_no_canonico_sin_revelar_bytes() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let parent = capability.identidad_parent.journal_identity().unwrap();
    let canonical = journal_prueba(parent).encode().unwrap();
    let malformed = b"malformed-journal-bytes-canary".to_vec();
    let noncanonical = [b" ".as_slice(), canonical.as_slice()].concat();

    for bytes in [malformed, noncanonical] {
        fs::write(nombre_journal(&root), &bytes).unwrap();
        let error = capability.cargar_canonico().unwrap_err();
        assert_eq!(
            error,
            ErrorJournalAnclado::InvalidJournal(JournalError::Malformed)
        );
        assert_eq!(error.to_string(), "journal_malformed");
        let debug = format!("{error:?}");
        let text = String::from_utf8_lossy(&bytes);
        assert!(!debug.contains(text.as_ref()));
        assert!(!error.to_string().contains(text.as_ref()));
        assert!(!debug.contains(root.to_string_lossy().as_ref()));
        assert!(!error.to_string().contains(root.to_string_lossy().as_ref()));
    }
}

#[test]
fn cargar_journal_rechaza_parent_distinto_sin_revelar_identidad() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let identity = capability.identidad_parent.journal_identity().unwrap();
    let wrong_parent = identidad_distinta(&identity);
    let bytes = journal_prueba(wrong_parent).encode().unwrap();
    fs::write(nombre_journal(&root), &bytes).unwrap();

    let error = capability.cargar_canonico().unwrap_err();

    assert_eq!(error, ErrorJournalAnclado::ParentIdentityMismatch);
    assert_eq!(error.to_string(), "journal_parent_identity_mismatch");
    let debug = format!("{error:?}");
    let text = String::from_utf8_lossy(&bytes);
    assert!(!debug.contains(root.to_string_lossy().as_ref()));
    assert!(!debug.contains(text.as_ref()));
    assert!(!error.to_string().contains(root.to_string_lossy().as_ref()));
    assert!(!error.to_string().contains(text.as_ref()));
}

#[test]
fn journal_canonico_lee_desde_basename_de_raiz_y_devuelve_identidad_y_bytes() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("knowledge.v2");
    fs::create_dir(&root).unwrap();
    let journal = nombre_journal(&root);
    let canary = b"canonical-journal-content";
    fs::write(&journal, canary).unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();

    let (identidad, bytes) = capability.abrir_y_leer_canonico().unwrap().unwrap();
    let identidad_esperada = super::identidad_archivo(&fs::File::open(&journal).unwrap()).unwrap();
    assert_eq!(identidad, identidad_esperada);
    assert_eq!(bytes, canary);
    assert_eq!(
        format!("{identidad:?}"),
        "IdentidadArchivoJournal([redacted])"
    );

    fs::write(&journal, b"").unwrap();
    let (identidad_vacia, bytes_vacios) = capability.abrir_y_leer_canonico().unwrap().unwrap();
    let identidad_vacia_esperada =
        super::identidad_archivo(&fs::File::open(&journal).unwrap()).unwrap();
    assert_eq!(identidad_vacia, identidad_vacia_esperada);
    assert_eq!(bytes_vacios, b"");
}

#[test]
fn crea_cierra_lee_desde_parent_anclado_y_conserva_identidad_del_handle() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let identidad_escrita = capability.crear_nuevo(b"journal-temp-canary").unwrap();

    let (identidad_leida, bytes) = capability.abrir_y_leer().unwrap();
    assert_eq!(identidad_escrita, identidad_leida);
    assert_eq!(bytes, b"journal-temp-canary");
    assert_eq!(fs::read(nombre_temporal(temp.path())).unwrap(), bytes);
    assert_eq!(
        format!("{identidad_leida:?}"),
        "IdentidadArchivoJournal([redacted])"
    );
}

#[test]
fn acepta_exactamente_max_journal_bytes_al_escribir_y_leer() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let bytes = vec![b'x'; MAX_JOURNAL_BYTES];

    let identidad_escrita = capability.crear_nuevo(&bytes).unwrap();
    let (identidad_leida, read) = capability.abrir_y_leer().unwrap();
    assert_eq!(identidad_escrita, identidad_leida);
    assert_eq!(read, bytes);

    let journal = nombre_journal(&root);
    fs::write(journal, &bytes).unwrap();
    let (_, canonical_read) = capability.abrir_y_leer_canonico().unwrap().unwrap();
    assert_eq!(canonical_read, bytes);
    assert_eq!(
        capability.cargar_canonico().unwrap_err(),
        ErrorJournalAnclado::InvalidJournal(JournalError::Malformed)
    );
}

#[test]
fn rechaza_max_mas_uno_antes_de_crear_y_al_leer() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let bytes = vec![b'x'; MAX_JOURNAL_BYTES + 1];
    let nombre = nombre_temporal(temp.path());

    assert_eq!(
        capability.crear_nuevo(&bytes).unwrap_err(),
        ErrorJournalAnclado::TooLarge
    );
    assert!(!nombre.exists());

    fs::write(&nombre, &bytes).unwrap();
    assert_eq!(
        capability.abrir_y_leer().unwrap_err(),
        ErrorJournalAnclado::TooLarge
    );
    assert_eq!(
        fs::metadata(nombre).unwrap().len(),
        (MAX_JOURNAL_BYTES + 1) as u64
    );

    fs::write(nombre_journal(&root), &bytes).unwrap();
    let error = capability.abrir_y_leer_canonico().unwrap_err();
    assert_eq!(error, ErrorJournalAnclado::TooLarge);
    assert_eq!(error.codigo(), "journal_too_large");
    assert_eq!(error.to_string(), "journal_too_large");
    assert!(!format!("{error:?}").contains("bundle"));
    assert_eq!(capability.cargar_canonico().unwrap_err(), error);
}

#[test]
fn codigos_de_error_journal_son_estables_y_redactados() {
    let casos = [
        (ErrorJournalAnclado::Io, "journal_io"),
        (ErrorJournalAnclado::Exists, "journal_exists"),
        (ErrorJournalAnclado::TooLarge, "journal_too_large"),
        (
            ErrorJournalAnclado::ReparseNotAllowed,
            "journal_reparse_not_allowed",
        ),
        (
            ErrorJournalAnclado::RegularFileRequired,
            "journal_regular_file_required",
        ),
        (ErrorJournalAnclado::ParentChanged, "journal_parent_changed"),
        (ErrorJournalAnclado::SourceMissing, "journal_source_missing"),
        (
            ErrorJournalAnclado::IdentityMismatch,
            "journal_identity_mismatch",
        ),
        (
            ErrorJournalAnclado::CanonicalConflict,
            "journal_canonical_conflict",
        ),
        (
            ErrorJournalAnclado::RemoteUnsupported,
            "journal_remote_unsupported",
        ),
        (
            ErrorJournalAnclado::ParentIdentityUnavailable,
            "journal_parent_identity_unavailable",
        ),
        (
            ErrorJournalAnclado::ParentIdentityMismatch,
            "journal_parent_identity_mismatch",
        ),
        (
            ErrorJournalAnclado::InvalidJournal(JournalError::Malformed),
            "journal_malformed",
        ),
    ];

    for (error, codigo) in casos {
        assert_eq!(error.codigo(), codigo);
        assert_eq!(error.to_string(), codigo);
        assert_eq!(
            format!("{error:?}"),
            format!("ErrorJournalAnclado({codigo:?})")
        );
    }
}

#[test]
fn colision_con_archivo_preserva_canary_y_rechaza_segundo_create() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let canary = b"first-create-canary";
    capability.crear_nuevo(canary).unwrap();

    assert_eq!(
        capability.crear_nuevo(b"replacement").unwrap_err(),
        ErrorJournalAnclado::Exists
    );
    let (_, read) = capability.abrir_y_leer().unwrap();
    assert_eq!(read, canary);
    assert_eq!(fs::read(nombre_temporal(temp.path())).unwrap(), canary);
}

#[test]
fn colision_con_directorio_no_lo_reemplaza_ni_sigue_y_preserva_canary() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let nombre = nombre_temporal(temp.path());
    fs::create_dir(&nombre).unwrap();
    fs::write(nombre.join("canary"), b"directory-canary").unwrap();

    assert_eq!(
        capability.crear_nuevo(b"replacement").unwrap_err(),
        ErrorJournalAnclado::Exists
    );
    assert_eq!(
        capability.abrir_y_leer().unwrap_err(),
        ErrorJournalAnclado::RegularFileRequired
    );
    assert_eq!(
        fs::read(nombre.join("canary")).unwrap(),
        b"directory-canary"
    );

    let journal = nombre_journal(&root);
    fs::create_dir(&journal).unwrap();
    fs::write(journal.join("canary"), b"canonical-directory-canary").unwrap();
    assert_eq!(
        capability.abrir_y_leer_canonico().unwrap_err(),
        ErrorJournalAnclado::RegularFileRequired
    );
    let error = capability.cargar_canonico().unwrap_err();
    assert_eq!(error, ErrorJournalAnclado::RegularFileRequired);
    assert_eq!(error.to_string(), "journal_regular_file_required");
    assert!(!format!("{error:?}").contains(root.to_string_lossy().as_ref()));
    assert!(!error.to_string().contains(root.to_string_lossy().as_ref()));
    assert_eq!(
        fs::read(journal.join("canary")).unwrap(),
        b"canonical-directory-canary"
    );
}

#[cfg(unix)]
#[test]
fn symlink_existente_no_se_reemplaza_ni_se_sigue_y_preserva_canary() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let target = temp.path().join("outside-canary");
    fs::write(&target, b"outside-canary").unwrap();
    let nombre = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    symlink(&target, &nombre).unwrap();
    symlink(&target, &journal).unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();

    assert_eq!(
        capability.crear_nuevo(b"replacement").unwrap_err(),
        ErrorJournalAnclado::Exists
    );
    assert_eq!(
        capability.abrir_y_leer().unwrap_err(),
        ErrorJournalAnclado::ReparseNotAllowed
    );
    assert_eq!(
        capability.abrir_y_leer_canonico().unwrap_err(),
        ErrorJournalAnclado::ReparseNotAllowed
    );
    let error = capability.cargar_canonico().unwrap_err();
    assert_eq!(error, ErrorJournalAnclado::ReparseNotAllowed);
    assert_eq!(error.to_string(), "journal_reparse_not_allowed");
    assert!(!format!("{error:?}").contains(root.to_string_lossy().as_ref()));
    assert!(!error.to_string().contains(root.to_string_lossy().as_ref()));
    assert_eq!(fs::read(&target).unwrap(), b"outside-canary");
}

#[cfg(windows)]
#[test]
fn junction_existente_no_se_reemplaza_ni_se_sigue_y_preserva_canary() {
    let temp = tempdir().unwrap();
    if crate::transaccion::anclado::renombrar_test_support::saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = crear_root(temp.path());
    let target = temp.path().join("outside-canary");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("canary"), b"outside-canary").unwrap();
    let nombre = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    for enlace in [&nombre, &journal] {
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(enlace)
            .arg(&target)
            .output()
            .expect("cmd crea junctions");
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
    }
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();

    assert_eq!(
        capability.crear_nuevo(b"replacement").unwrap_err(),
        ErrorJournalAnclado::Exists
    );
    assert!(matches!(
        capability.abrir_y_leer().unwrap_err(),
        ErrorJournalAnclado::ReparseNotAllowed | ErrorJournalAnclado::RegularFileRequired
    ));
    assert_eq!(
        capability.abrir_y_leer_canonico().unwrap_err(),
        ErrorJournalAnclado::ReparseNotAllowed
    );
    let error = capability.cargar_canonico().unwrap_err();
    assert_eq!(error, ErrorJournalAnclado::ReparseNotAllowed);
    assert_eq!(error.to_string(), "journal_reparse_not_allowed");
    assert!(!format!("{error:?}").contains(root.to_string_lossy().as_ref()));
    assert!(!error.to_string().contains(root.to_string_lossy().as_ref()));
    assert_eq!(fs::read(target.join("canary")).unwrap(), b"outside-canary");
}

#[cfg(windows)]
#[test]
fn raiz_parent_windows_deriva_drive_y_unc_sin_probar_acceso_real() {
    assert_eq!(
        super::super::raiz_parent_windows(Path::new(r"C:\")).unwrap(),
        Path::new(r"C:\")
    );
    assert_eq!(
        super::super::raiz_parent_windows(Path::new(r"\\server\share")).unwrap(),
        Path::new(r"\\server\share\")
    );
}
