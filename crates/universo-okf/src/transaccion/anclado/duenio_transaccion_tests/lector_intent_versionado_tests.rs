use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
use crate::transaccion::anclado::identidad_archivo::{IdentidadArchivo, identidad_archivo};
use crate::transaccion::anclado::intent_persistido::{self, IntentRetiroPersistidoVersionado};
use crate::transaccion::anclado::intent_persistido_v2::IntentRetiroPersistidoV2;
use crate::transaccion::anclado::{IdentidadDirectorio, ScopeFilesystem, scope_filesystem};
use crate::transaccion::journal::v2::{JournalKind, JournalV2};
use crate::transaccion::journal::{
    ArtifactFingerprint, DirectoryIdentity, Journal, JournalState, RawTreeDigest,
};
use sha2::{Digest, Sha256};
use tempfile::{TempDir, tempdir};

use super::{
    CandidatoIntentConVinculosCoincidentes, CandidatoIntentDecodificadoSinVinculo,
    ErrorCandidatoIntentSinVinculo, ErrorIntentVersionadoVinculado, ErrorLecturaFrameIntent,
    comparar_sonda_visible_intent, leer_acotado, leer_candidato_intent_versionado,
    leer_candidato_intent_versionado_con_cap_y_hook, leer_frame_intent, leer_frame_intent_con_hook,
    leer_intent_versionado_vinculado,
};

struct ArchivoFixture {
    _directorio: TempDir,
    ruta: PathBuf,
    archivo: File,
    bytes: Vec<u8>,
    identidad: IdentidadArchivo,
    scope: ScopeFilesystem,
}

fn archivo_fixture(bytes: &[u8]) -> ArchivoFixture {
    let directorio = tempdir().unwrap();
    let ruta = directorio.path().join("intent-frame");
    let mut archivo = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&ruta)
        .unwrap();
    archivo.write_all(bytes).unwrap();
    archivo.seek(SeekFrom::End(0)).unwrap();
    let identidad = identidad_archivo(&archivo).unwrap();
    let scope = scope_filesystem(&archivo).unwrap();
    ArchivoFixture {
        _directorio: directorio,
        ruta,
        archivo,
        bytes: bytes.to_vec(),
        identidad,
        scope,
    }
}

#[test]
fn acepta_el_cap_exacto_y_rebobina_el_handle_original() {
    let fixture = archivo_fixture(b"frame with cursor already at eof");
    let cap = fixture.bytes.len();
    let observado = leer_frame_intent(fixture.archivo, fixture.scope, cap).unwrap();

    assert_eq!(observado.bytes, fixture.bytes);
    assert!(observado.identidad_archivo == fixture.identidad);
    assert!(observado.scope == fixture.scope);
    assert_eq!(
        observado.archivo_original.metadata().unwrap().len(),
        cap as u64
    );
    assert_eq!(format!("{observado:?}"), "FrameIntentLeido { .. }");
    assert_eq!(fs::read(&fixture.ruta).unwrap(), fixture.bytes);
}

#[test]
fn metadata_cap_mas_uno_rechaza_antes_del_hook_y_preserva_bytes() {
    let fixture = archivo_fixture(b"frame");
    let hook_llamado = std::cell::Cell::new(false);
    let error = leer_frame_intent_con_hook(
        fixture.archivo,
        fixture.scope,
        fixture.bytes.len() - 1,
        || hook_llamado.set(true),
    )
    .unwrap_err();

    assert_eq!(error, ErrorLecturaFrameIntent::FrameTooLarge);
    assert!(!hook_llamado.get());
    assert_eq!(fs::read(&fixture.ruta).unwrap(), fixture.bytes);
}

#[test]
fn crecimiento_despues_de_metadata_falla_por_el_byte_sentinela() {
    let fixture = archivo_fixture(b"frame");
    let cap = fixture.bytes.len();
    let mutation = fixture.archivo.try_clone().unwrap();
    let error = leer_frame_intent_con_hook(fixture.archivo, fixture.scope, cap, || {
        mutation.set_len((cap + 1) as u64).unwrap()
    })
    .unwrap_err();

    assert_eq!(error, ErrorLecturaFrameIntent::FrameTooLarge);
    let bytes = fs::read(&fixture.ruta).unwrap();
    assert_eq!(bytes.len(), cap + 1);
    assert_eq!(&bytes[..cap], fixture.bytes);
}

#[test]
fn scope_distinto_del_esperado_se_rechaza_sin_invocar_hook() {
    let fixture = archivo_fixture(b"frame");
    let hook_llamado = std::cell::Cell::new(false);
    let error = leer_frame_intent_con_hook(
        fixture.archivo,
        ScopeFilesystem(fixture.scope.0 ^ 1),
        fixture.bytes.len(),
        || hook_llamado.set(true),
    )
    .unwrap_err();

    assert_eq!(error, ErrorLecturaFrameIntent::ScopeMismatch);
    assert!(!hook_llamado.get());
    assert_eq!(fs::read(&fixture.ruta).unwrap(), fixture.bytes);
}

struct InterruptedOnce<'a> {
    cursor: Cursor<&'a [u8]>,
    interrupted: bool,
}

impl Read for InterruptedOnce<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.cursor.read(buffer)
    }
}

struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("private file path and contents"))
    }
}

#[test]
fn colector_reintenta_interrupted_y_redacta_error_de_lectura() {
    let expected = b"fragmented intent frame";
    let cap_mas_uno = u64::try_from(expected.len() + 1).unwrap();
    let mut interrupted = InterruptedOnce {
        cursor: Cursor::new(expected),
        interrupted: false,
    };
    let bytes = leer_acotado(&mut interrupted, expected.len(), cap_mas_uno).unwrap();
    assert_eq!(bytes, expected);

    let error = leer_acotado(&mut FailingReader, 8, 9).unwrap_err();
    assert_eq!(error, ErrorLecturaFrameIntent::ReadIo);
    assert!(!format!("{error:?}").contains("private file path"));
}

#[test]
fn cap_sin_capacidad_para_el_sentinela_falla_antes_de_leer() {
    let fixture = archivo_fixture(b"frame");
    let error = leer_frame_intent(fixture.archivo, fixture.scope, usize::MAX).unwrap_err();

    assert_eq!(error, ErrorLecturaFrameIntent::LengthOverflow);
    assert_eq!(fs::read(&fixture.ruta).unwrap(), fixture.bytes);
}

const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
const CONTEXT_BYTES: &[u8] = b"; versioned intent fixture\n";

#[derive(Clone, Copy)]
enum VersionIntentFixture {
    V1,
    V2,
}

struct IntentFixture {
    _directorio: TempDir,
    ruta_intent: PathBuf,
    ruta_journal: PathBuf,
    ruta_contexto: PathBuf,
    archivo_intent: File,
    bytes_intent: Vec<u8>,
    bytes_journal: Vec<u8>,
    bytes_contexto: Vec<u8>,
    identidad_intent: IdentidadArchivo,
    identidad_parent: IdentidadDirectorio,
    parent_journal: DirectoryIdentity,
    scope: ScopeFilesystem,
}

#[derive(Clone, Copy)]
enum BindingIncorrecto {
    Ninguno,
    IdentidadIntent,
    Scope,
    Parent,
}

fn directory_identity(value: u64) -> DirectoryIdentity {
    #[cfg(unix)]
    {
        DirectoryIdentity::unix(17, value)
    }
    #[cfg(windows)]
    {
        DirectoryIdentity::windows(17, format!("{value:032x}"))
    }
}

fn another_directory_identity(identity: &DirectoryIdentity) -> DirectoryIdentity {
    #[cfg(unix)]
    if let DirectoryIdentity::Unix { device, inode } = identity {
        return DirectoryIdentity::unix(*device, inode.wrapping_add(1));
    }
    #[cfg(windows)]
    if let DirectoryIdentity::Windows { volume, file_id } = identity {
        let alternate = if file_id == "00000000000000000000000000000000" {
            "11111111111111111111111111111111"
        } else {
            "00000000000000000000000000000000"
        };
        return DirectoryIdentity::windows(*volume, alternate);
    }
    unreachable!("la identidad de directorio debe pertenecer a esta plataforma")
}

fn fingerprint(value: u64) -> ArtifactFingerprint {
    ArtifactFingerprint::new(
        directory_identity(value),
        RawTreeDigest::from_digest([u8::try_from(value).unwrap(); 32]),
    )
}

fn journal_v1(parent: DirectoryIdentity) -> Journal {
    let mut journal =
        Journal::new(TRANSACTION_ID, parent, fingerprint(31), fingerprint(32)).unwrap();
    for state in [
        JournalState::BackupReady,
        JournalState::Installed,
        JournalState::Committed,
    ] {
        journal.advance(state).unwrap();
    }
    journal
}

fn journal_v2(parent: DirectoryIdentity) -> JournalV2 {
    let mut journal =
        JournalV2::new(TRANSACTION_ID, parent, fingerprint(41), fingerprint(42)).unwrap();
    for state in [
        JournalState::BackupReady,
        JournalState::Installed,
        JournalState::Committed,
    ] {
        journal.advance(state).unwrap();
    }
    journal
}

fn open_empty(parent: &std::path::Path, name: &str) -> (PathBuf, File) {
    let path = parent.join(name);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    (path, file)
}

fn intent_fixture(version: VersionIntentFixture) -> IntentFixture {
    intent_fixture_con_binding(version, BindingIncorrecto::Ninguno)
}

fn intent_fixture_con_binding(
    version: VersionIntentFixture,
    binding_incorrecto: BindingIncorrecto,
) -> IntentFixture {
    let directorio = tempdir().unwrap();
    let parent_handle =
        super::super::super::abrir_directorio_absoluto(directorio.path(), false).unwrap();
    let identidad_parent = super::super::super::metadata_directorio(&parent_handle).unwrap();
    let parent_journal = identidad_parent.journal_identity().unwrap();

    let (ruta_intent, mut archivo_intent) = open_empty(directorio.path(), "intent-frame");
    let (ruta_journal, mut archivo_journal) = open_empty(directorio.path(), "journal-record");
    let (ruta_contexto, mut archivo_contexto) = open_empty(directorio.path(), "context-artifact");
    archivo_contexto.write_all(CONTEXT_BYTES).unwrap();

    let identidad_intent = identidad_archivo(&archivo_intent).unwrap();
    let identidad_journal = identidad_archivo(&archivo_journal).unwrap();
    let identidad_contexto = identidad_archivo(&archivo_contexto).unwrap();
    let scope = scope_filesystem(&archivo_intent).unwrap();
    let identidad_intent_persistida = match binding_incorrecto {
        BindingIncorrecto::IdentidadIntent => identidad_journal,
        _ => identidad_intent,
    };
    let scope_persistido = match binding_incorrecto {
        BindingIncorrecto::Scope => scope.0 ^ 1,
        _ => scope.0,
    };
    let parent_persistido = match binding_incorrecto {
        BindingIncorrecto::Parent => another_directory_identity(&parent_journal),
        _ => parent_journal.clone(),
    };
    let digest_contexto: [u8; 32] = Sha256::digest(CONTEXT_BYTES).into();
    let longitud_contexto = u64::try_from(CONTEXT_BYTES.len()).unwrap();

    let (bytes_journal, bytes_intent) = match version {
        VersionIntentFixture::V1 => {
            let journal = journal_v1(parent_persistido);
            let bytes_journal = journal.encode().unwrap();
            archivo_journal.write_all(&bytes_journal).unwrap();
            let bytes_intent = intent_persistido::codificar_nuevo(
                &journal,
                identidad_intent_persistida,
                identidad_journal,
                identidad_contexto,
                scope_persistido,
                longitud_contexto,
                digest_contexto,
            )
            .unwrap();
            (bytes_journal, bytes_intent)
        }
        VersionIntentFixture::V2 => {
            let journal = journal_v2(parent_persistido);
            let bytes_journal = journal.encode().unwrap();
            archivo_journal.write_all(&bytes_journal).unwrap();
            let intent_id =
                IdentidadArchivoPersistida::try_from(identidad_intent_persistida).unwrap();
            let journal_id = IdentidadArchivoPersistida::try_from(identidad_journal).unwrap();
            let context_id = IdentidadArchivoPersistida::try_from(identidad_contexto).unwrap();
            let bytes_intent = IntentRetiroPersistidoV2::desde_datos_persistidos(
                journal,
                intent_id,
                journal_id,
                context_id,
                scope_persistido,
                longitud_contexto,
                digest_contexto,
            )
            .unwrap()
            .encode()
            .unwrap();
            (bytes_journal, bytes_intent)
        }
    };
    archivo_intent.write_all(&bytes_intent).unwrap();
    archivo_intent.seek(SeekFrom::End(0)).unwrap();

    IntentFixture {
        _directorio: directorio,
        ruta_intent,
        ruta_journal,
        ruta_contexto,
        archivo_intent,
        bytes_intent,
        bytes_journal,
        bytes_contexto: CONTEXT_BYTES.to_vec(),
        identidad_intent,
        identidad_parent,
        parent_journal,
        scope,
    }
}

fn assert_preserved(
    ruta_intent: &std::path::Path,
    ruta_journal: &std::path::Path,
    ruta_contexto: &std::path::Path,
    bytes_intent: &[u8],
    bytes_journal: &[u8],
    bytes_contexto: &[u8],
) {
    assert_eq!(fs::read(ruta_intent).unwrap(), bytes_intent);
    assert_eq!(fs::read(ruta_journal).unwrap(), bytes_journal);
    assert_eq!(fs::read(ruta_contexto).unwrap(), bytes_contexto);
}

fn assert_candidate_matches(
    candidate: &CandidatoIntentDecodificadoSinVinculo,
    expected: &IntentRetiroPersistidoVersionado,
    version: VersionIntentFixture,
) {
    assert!(&candidate.envelope == expected);
    match (&candidate.envelope, version) {
        (IntentRetiroPersistidoVersionado::V1(intent), VersionIntentFixture::V1) => {
            assert!(intent.journal().state() == JournalState::Committed);
        }
        (IntentRetiroPersistidoVersionado::V2(intent), VersionIntentFixture::V2) => {
            assert!(intent.journal().state() == JournalState::Committed);
            assert!(intent.journal().kind() == JournalKind::ContextRequired);
        }
        _ => panic!("el candidato conserva la variante decodificada"),
    }
}

fn assert_linked_matches(
    candidate: &CandidatoIntentConVinculosCoincidentes,
    expected: &IntentRetiroPersistidoVersionado,
    version: VersionIntentFixture,
) {
    assert!(&candidate.envelope == expected);
    match (&candidate.envelope, version) {
        (IntentRetiroPersistidoVersionado::V1(intent), VersionIntentFixture::V1) => {
            assert!(intent.journal().state() == JournalState::Committed);
        }
        (IntentRetiroPersistidoVersionado::V2(intent), VersionIntentFixture::V2) => {
            assert!(intent.journal().state() == JournalState::Committed);
            assert!(intent.journal().kind() == JournalKind::ContextRequired);
        }
        _ => panic!("el vínculo conserva la variante del envelope"),
    }
}

#[test]
fn decodifica_candidato_v1_y_v2_con_cap_exacta_y_conserva_archivo() {
    for version in [VersionIntentFixture::V1, VersionIntentFixture::V2] {
        let fixture = intent_fixture(version);
        let IntentFixture {
            _directorio,
            ruta_intent,
            ruta_journal,
            ruta_contexto,
            archivo_intent,
            bytes_intent,
            bytes_journal,
            bytes_contexto,
            identidad_intent,
            scope,
            ..
        } = fixture;
        let cap = bytes_intent.len();
        let expected = intent_persistido::decodificar_versionado(&bytes_intent).unwrap();
        let candidate =
            leer_candidato_intent_versionado_con_cap_y_hook(archivo_intent, scope, cap, || {})
                .unwrap();

        assert_candidate_matches(&candidate, &expected, version);
        assert!(identidad_archivo(&candidate.archivo_original).unwrap() == identidad_intent);
        assert_eq!(
            candidate.archivo_original.metadata().unwrap().len(),
            u64::try_from(cap).unwrap()
        );
        assert_preserved(
            &ruta_intent,
            &ruta_journal,
            &ruta_contexto,
            &bytes_intent,
            &bytes_journal,
            &bytes_contexto,
        );
    }
}

#[test]
fn limite_de_prueba_se_clampa_a_la_cuota_familiar() {
    let cap_familiar = intent_persistido::cuota_maxima_frame_bytes();
    let bytes = vec![b'x'; cap_familiar + 1];
    let fixture = archivo_fixture(&bytes);
    let hook_llamado = std::cell::Cell::new(false);
    let error = match leer_candidato_intent_versionado_con_cap_y_hook(
        fixture.archivo,
        fixture.scope,
        usize::MAX,
        || hook_llamado.set(true),
    ) {
        Ok(_) => panic!("la cuota de prueba no debe superar la cuota familiar"),
        Err(error) => error,
    };

    assert_eq!(
        error,
        ErrorCandidatoIntentSinVinculo::Frame(ErrorLecturaFrameIntent::FrameTooLarge)
    );
    assert!(!hook_llamado.get());
    assert_eq!(fs::read(&fixture.ruta).unwrap(), bytes);
}

#[test]
fn wrapper_productivo_usa_la_cuota_familiar_sin_exponerla_al_caller() {
    let fixture = intent_fixture(VersionIntentFixture::V1);
    let IntentFixture {
        _directorio,
        ruta_intent,
        ruta_journal,
        ruta_contexto,
        archivo_intent,
        bytes_intent,
        bytes_journal,
        bytes_contexto,
        scope,
        ..
    } = fixture;
    let expected = intent_persistido::decodificar_versionado(&bytes_intent).unwrap();
    let candidate = leer_candidato_intent_versionado(archivo_intent, scope).unwrap();

    assert_candidate_matches(&candidate, &expected, VersionIntentFixture::V1);
    assert_eq!(
        candidate.archivo_original.metadata().unwrap().len(),
        u64::try_from(bytes_intent.len()).unwrap()
    );
    assert_preserved(
        &ruta_intent,
        &ruta_journal,
        &ruta_contexto,
        &bytes_intent,
        &bytes_journal,
        &bytes_contexto,
    );
}

#[test]
fn vincula_envelope_v1_y_v2_con_id_scope_parent_y_handle_originales() {
    for version in [VersionIntentFixture::V1, VersionIntentFixture::V2] {
        let fixture = intent_fixture(version);
        let IntentFixture {
            _directorio,
            ruta_intent,
            ruta_journal,
            ruta_contexto,
            archivo_intent,
            bytes_intent,
            bytes_journal,
            bytes_contexto,
            identidad_intent,
            identidad_parent,
            parent_journal,
            scope,
        } = fixture;
        let expected = intent_persistido::decodificar_versionado(&bytes_intent).unwrap();
        let linked =
            leer_intent_versionado_vinculado(archivo_intent, scope, identidad_parent).unwrap();

        assert_linked_matches(&linked, &expected, version);
        assert!(linked.identidad_archivo == identidad_intent);
        assert!(linked.scope == scope);
        assert!(linked.parent_journal == parent_journal);
        assert!(identidad_archivo(&linked.archivo_original).unwrap() == identidad_intent);
        assert_preserved(
            &ruta_intent,
            &ruta_journal,
            &ruta_contexto,
            &bytes_intent,
            &bytes_journal,
            &bytes_contexto,
        );
    }
}

fn assert_binding_error(fixture: IntentFixture, expected: ErrorIntentVersionadoVinculado) {
    let IntentFixture {
        _directorio,
        ruta_intent,
        ruta_journal,
        ruta_contexto,
        archivo_intent,
        bytes_intent,
        bytes_journal,
        bytes_contexto,
        identidad_parent,
        scope,
        ..
    } = fixture;
    let error = match leer_intent_versionado_vinculado(archivo_intent, scope, identidad_parent) {
        Ok(_) => panic!("el vínculo incompatible debe rechazarse"),
        Err(error) => error,
    };

    assert!(error == expected);
    assert_eq!(format!("{error:?}"), expected.codigo());
    assert_eq!(error.to_string(), expected.codigo());
    assert_preserved(
        &ruta_intent,
        &ruta_journal,
        &ruta_contexto,
        &bytes_intent,
        &bytes_journal,
        &bytes_contexto,
    );
}

#[test]
fn rechaza_id_y_scope_persistidos_distintos_para_v1_y_v2() {
    for version in [VersionIntentFixture::V1, VersionIntentFixture::V2] {
        assert_binding_error(
            intent_fixture_con_binding(version, BindingIncorrecto::IdentidadIntent),
            ErrorIntentVersionadoVinculado::SourceBindingMismatch,
        );
        assert_binding_error(
            intent_fixture_con_binding(version, BindingIncorrecto::Scope),
            ErrorIntentVersionadoVinculado::SourceBindingMismatch,
        );
    }
}

#[test]
fn rechaza_parent_persistido_distinto_para_v1_y_v2() {
    for version in [VersionIntentFixture::V1, VersionIntentFixture::V2] {
        assert_binding_error(
            intent_fixture_con_binding(version, BindingIncorrecto::Parent),
            ErrorIntentVersionadoVinculado::ParentBindingMismatch,
        );
    }
}

#[test]
fn compara_sonda_relativa_con_id_y_scope_retenidos_para_v1_y_v2() {
    for version in [VersionIntentFixture::V1, VersionIntentFixture::V2] {
        let fixture = intent_fixture(version);
        let parent = super::super::super::abrir_directorio_absoluto(
            fixture.ruta_intent.parent().unwrap(),
            true,
        )
        .unwrap();
        let name = fixture.ruta_intent.file_name().unwrap();
        let source = super::super::lector_intent_canonico::abrir_intent_canonico(&parent, name)
            .unwrap()
            .unwrap();
        let candidate =
            leer_intent_versionado_vinculado(source, fixture.scope, fixture.identidad_parent)
                .unwrap();
        let probe = super::super::lector_intent_canonico::abrir_intent_canonico(&parent, name)
            .unwrap()
            .unwrap();

        assert_eq!(comparar_sonda_visible_intent(&candidate, &probe), Ok(()));
        assert_eq!(
            fs::read(&fixture.ruta_intent).unwrap(),
            fixture.bytes_intent
        );
        drop(probe);
        drop(candidate);
    }
}

#[test]
fn reemplazo_visible_con_bytes_iguales_e_id_distinto_se_rechaza_y_preserva() {
    for version in [VersionIntentFixture::V1, VersionIntentFixture::V2] {
        let fixture = intent_fixture(version);
        let IntentFixture {
            _directorio,
            ruta_intent,
            archivo_intent,
            bytes_intent,
            identidad_parent,
            scope,
            ..
        } = fixture;
        drop(archivo_intent);
        let retained = ruta_intent.with_file_name("retained-intent-frame");
        let name = ruta_intent.file_name().unwrap();
        let parent =
            super::super::super::abrir_directorio_absoluto(ruta_intent.parent().unwrap(), true)
                .unwrap();
        let source = super::super::lector_intent_canonico::abrir_intent_canonico(&parent, name)
            .unwrap()
            .unwrap();
        let candidate = leer_intent_versionado_vinculado(source, scope, identidad_parent).unwrap();
        let original_identity = candidate.identidad_archivo;

        fs::rename(&ruta_intent, &retained).unwrap();
        fs::write(&ruta_intent, &bytes_intent).unwrap();
        let probe = super::super::lector_intent_canonico::abrir_intent_canonico(&parent, name)
            .unwrap()
            .unwrap();
        let replacement_identity = identidad_archivo(&probe).unwrap();
        assert!(replacement_identity != candidate.identidad_archivo);
        assert_eq!(
            comparar_sonda_visible_intent(&candidate, &probe),
            Err(ErrorIntentVersionadoVinculado::SourceBindingMismatch)
        );

        drop(probe);
        drop(candidate);
        drop(parent);
        assert_eq!(fs::read(&retained).unwrap(), bytes_intent);
        assert_eq!(fs::read(&ruta_intent).unwrap(), bytes_intent);
        let retained_identity = identidad_archivo(&File::open(&retained).unwrap()).unwrap();
        let visible_identity = identidad_archivo(&File::open(&ruta_intent).unwrap()).unwrap();
        assert!(retained_identity != visible_identity);
        assert_eq!(retained_identity, original_identity);
        assert_eq!(visible_identity, replacement_identity);
    }
}

#[test]
fn rechaza_scope_retenido_distinto_de_la_sonda_para_v1_y_v2() {
    for version in [VersionIntentFixture::V1, VersionIntentFixture::V2] {
        let fixture = intent_fixture(version);
        let parent = super::super::super::abrir_directorio_absoluto(
            fixture.ruta_intent.parent().unwrap(),
            true,
        )
        .unwrap();
        let name = fixture.ruta_intent.file_name().unwrap();
        let source = super::super::lector_intent_canonico::abrir_intent_canonico(&parent, name)
            .unwrap()
            .unwrap();
        let mut candidate =
            leer_intent_versionado_vinculado(source, fixture.scope, fixture.identidad_parent)
                .unwrap();
        candidate.scope = ScopeFilesystem(candidate.scope.0 ^ 1);
        let probe = super::super::lector_intent_canonico::abrir_intent_canonico(&parent, name)
            .unwrap()
            .unwrap();

        assert_eq!(
            comparar_sonda_visible_intent(&candidate, &probe),
            Err(ErrorIntentVersionadoVinculado::SourceBindingMismatch)
        );
        assert_preserved(
            &fixture.ruta_intent,
            &fixture.ruta_journal,
            &fixture.ruta_contexto,
            &fixture.bytes_intent,
            &fixture.bytes_journal,
            &fixture.bytes_contexto,
        );
    }
}

#[test]
fn rechaza_handle_de_directorio_como_sonda_para_v1_y_v2() {
    for version in [VersionIntentFixture::V1, VersionIntentFixture::V2] {
        let fixture = intent_fixture(version);
        let parent = super::super::super::abrir_directorio_absoluto(
            fixture.ruta_intent.parent().unwrap(),
            true,
        )
        .unwrap();
        let name = fixture.ruta_intent.file_name().unwrap();
        let source = super::super::lector_intent_canonico::abrir_intent_canonico(&parent, name)
            .unwrap()
            .unwrap();
        let candidate =
            leer_intent_versionado_vinculado(source, fixture.scope, fixture.identidad_parent)
                .unwrap();

        assert_eq!(
            comparar_sonda_visible_intent(&candidate, &parent),
            Err(ErrorIntentVersionadoVinculado::SourceIdentityUnverified)
        );
        assert_preserved(
            &fixture.ruta_intent,
            &fixture.ruta_journal,
            &fixture.ruta_contexto,
            &fixture.bytes_intent,
            &fixture.bytes_journal,
            &fixture.bytes_contexto,
        );
    }
}
