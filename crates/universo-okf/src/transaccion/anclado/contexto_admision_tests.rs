use super::*;

use crate::transaccion::admision::envelope::{
    EnvelopeContextoAdmision, codec, tests as envelope_fixtures,
};
use crate::transaccion::anclado::contexto_admision::ResultadoEliminacionContextoAnclado;
use crate::transaccion::anclado::journal_temporal::ResultadoPublicacionJournal;
use crate::transaccion::journal::{
    ArtifactFingerprint, DirectoryIdentity, Journal, JournalState, RawTreeDigest,
};
use crate::transaccion::lock::{LockEscritura, serializar_forks_y_reacquisiciones_test};
use crate::transaccion::raiz::UbicacionRaiz;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::path::Path;
use tempfile::tempdir;

#[path = "contexto_admision_tests/contexto_canonico_tests.rs"]
mod contexto_canonico_tests;
#[path = "contexto_admision_tests/eliminar_contexto_resultados_tests.rs"]
mod eliminar_contexto_resultados_tests;
#[path = "contexto_admision_tests/eliminar_contexto_tests.rs"]
mod eliminar_contexto_tests;
#[path = "contexto_admision_tests/lector_versionado_invalid_tests.rs"]
mod lector_versionado_invalid_tests;
#[path = "contexto_admision_tests/lector_versionado_tests.rs"]
mod lector_versionado_tests;

fn frame(transaction_id: &str, parent: DirectoryIdentity) -> Vec<u8> {
    codec::codificar(&envelope(transaction_id, parent)).unwrap()
}

pub(super) fn envelope(
    transaction_id: &str,
    parent: DirectoryIdentity,
) -> EnvelopeContextoAdmision {
    // Storage fixture con parent real; las fingerprints sintéticas no prueban admisión.
    let old = ArtifactFingerprint::new(
        identity(2),
        RawTreeDigest::from_hex("a".repeat(64)).unwrap(),
    );
    let new = ArtifactFingerprint::new(
        identity(3),
        RawTreeDigest::from_hex("b".repeat(64)).unwrap(),
    );
    let journal = Journal::new(transaction_id, parent, old, new).unwrap();
    EnvelopeContextoAdmision::ligar(envelope_fixtures::contexto(b"\n"), &journal)
}

fn identity(seed: u64) -> DirectoryIdentity {
    #[cfg(unix)]
    {
        DirectoryIdentity::unix(seed, seed + 1)
    }
    #[cfg(windows)]
    {
        DirectoryIdentity::windows(seed, format!("{seed:032x}"))
    }
}

fn ubicacion(parent: &Path, crear_root: bool) -> (UbicacionRaiz, std::path::PathBuf) {
    fs::create_dir_all(parent).unwrap();
    let root = parent.join("bundle");
    if crear_root {
        fs::create_dir(&root).unwrap();
    }
    (UbicacionRaiz::nueva(&root).unwrap(), root)
}

fn contexto_path(ubicacion: &UbicacionRaiz) -> std::path::PathBuf {
    let parent = ubicacion.ruta().parent().unwrap();
    parent.join(nombre_canonico(ubicacion.ruta().file_name().unwrap()).unwrap())
}

fn temporal_contexto_path(ubicacion: &UbicacionRaiz) -> std::path::PathBuf {
    let parent = ubicacion.ruta().parent().unwrap();
    let mut nombre = nombre_canonico(ubicacion.ruta().file_name().unwrap()).unwrap();
    nombre.push(".tmp");
    parent.join(nombre)
}

fn crear_recibo<'lease>(
    owner: &'lease mut super::super::duenio_transaccion::DuenioTransaccionAnclado<'_>,
    envelope: &EnvelopeContextoAdmision,
) -> super::temporal::ReciboContextoTemporal<'lease> {
    owner
        .contexto_admision()
        .unwrap()
        .crear_temporal(envelope)
        .unwrap()
}

fn parent_identity(path: &Path) -> DirectoryIdentity {
    let parent = super::super::abrir_directorio_absoluto(path, true).unwrap();
    metadata_directorio(&parent)
        .unwrap()
        .journal_identity()
        .unwrap()
}

#[test]
fn reabre_el_contexto_despues_de_soltar_y_readquirir_el_lock_en_el_proceso() {
    // Coordina el drop/reacquire con los forks de otros tests.
    let _serializacion = serializar_forks_y_reacquisiciones_test();
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, false);
    let esperado = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    fs::write(contexto_path(&ubicacion), &esperado).unwrap();
    {
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        let mut owner =
            super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
                .unwrap();
        let lease = owner.contexto_admision().unwrap();
        let (envelope, _) = lease.leer_canonico().unwrap().unwrap();
        assert_eq!(codec::codificar(&envelope).unwrap(), esperado);
    }
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (envelope, _) = lease.leer_canonico().unwrap().unwrap();

    assert_eq!(codec::codificar(&envelope).unwrap(), esperado);
}

#[test]
fn publica_y_reabre_contexto_y_journal_prepared_para_replay_puro() {
    // Coordina el drop/reacquire con los forks de otros tests.
    let _serializacion = serializar_forks_y_reacquisiciones_test();
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let contexto = envelope_fixtures::contexto(b"\n");
    let fixture = envelope_fixtures::journal_valido(&contexto);
    let journal = Journal::new(
        fixture.transaction_id(),
        parent_identity(&parent),
        fixture.old().clone(),
        fixture.new_fingerprint().clone(),
    )
    .unwrap();
    let envelope = EnvelopeContextoAdmision::ligar(contexto, &journal);
    let bytes_contexto = codec::codificar(&envelope).unwrap();
    let bytes_journal = journal.encode().unwrap();
    let ruta_contexto = contexto_path(&ubicacion);
    let ruta_journal = ubicacion.journal().as_path().to_path_buf();

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let id_contexto_publicado = {
        let temporal = crear_recibo(&mut owner, &envelope);
        let identidad = temporal.datos.identidad_archivo;
        let publicado = temporal.verificar().unwrap().publicar().unwrap();
        assert_eq!(format!("{publicado:?}"), "ReciboContextoPublicado { .. }");
        identidad
    };
    let id_journal_publicado = {
        let lease = owner.journal_temporal().unwrap();
        let identidad = lease.crear_nuevo(&bytes_journal).unwrap();
        assert!(matches!(
            lease.publicar(identidad, &journal),
            ResultadoPublicacionJournal::Confirmado { .. }
        ));
        identidad
    };
    drop(owner);
    drop(lock);

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let (id_journal_reabierto, journal_reabierto, bytes_journal_reabiertos) = {
        let lease = owner.journal_temporal().unwrap();
        let (id_bytes, bytes) = lease.abrir_y_leer_canonico().unwrap().unwrap();
        let (id_record, record) = lease.cargar_canonico().unwrap().unwrap();
        assert_eq!(id_bytes, id_record);
        (id_record, record, bytes)
    };
    let (envelope_reabierto, id_contexto_reabierto) = {
        let lease = owner.contexto_admision().unwrap();
        lease.leer_canonico().unwrap().unwrap()
    };
    let esperado = envelope_reabierto
        .validar_replay(
            &journal_reabierto,
            envelope_fixtures::MOTOR,
            envelope_fixtures::POLICY,
        )
        .unwrap();

    assert_eq!(journal_reabierto, journal);
    assert_eq!(journal_reabierto.state(), JournalState::Prepared);
    assert_eq!(id_journal_reabierto, id_journal_publicado);
    assert_eq!(bytes_journal_reabiertos, bytes_journal);
    assert_eq!(id_contexto_reabierto, id_contexto_publicado);
    assert_eq!(esperado.transaction_id(), journal.transaction_id());
    assert!(esperado.parent() == journal.parent());
    assert!(esperado.old() == journal.old());
    assert!(esperado.new_fingerprint() == journal.new_fingerprint());
    assert!(!esperado.entradas().is_empty());

    let identidad_journal_antes_motor =
        identidad_archivo(&File::open(&ruta_journal).unwrap()).unwrap();
    for (motor, policy) in [
        ("engine-unrecognized/1", envelope_fixtures::POLICY),
        (envelope_fixtures::MOTOR, "policy-unrecognized/1"),
    ] {
        let error = envelope_reabierto
            .validar_replay(&journal_reabierto, motor, policy)
            .unwrap_err();
        assert_eq!(error.codigo(), "replay_engine_policy_mismatch");
    }
    assert_eq!(fs::read(&ruta_contexto).unwrap(), bytes_contexto);
    assert_eq!(fs::read(&ruta_journal).unwrap(), bytes_journal);
    assert_eq!(
        identidad_archivo(&File::open(&ruta_contexto).unwrap()).unwrap(),
        id_contexto_reabierto
    );
    assert_eq!(
        identidad_archivo(&File::open(&ruta_journal).unwrap()).unwrap(),
        identidad_journal_antes_motor
    );

    let journal_cambiado = Journal::new(
        "1123456789abcdef0123456789abcdef",
        journal.parent().clone(),
        journal.old().clone(),
        journal.new_fingerprint().clone(),
    )
    .unwrap();
    let bytes_journal_cambiado = journal_cambiado.encode().unwrap();
    fs::write(&ruta_journal, &bytes_journal_cambiado).unwrap();
    let (id_journal_cambiado, journal_cambiado, bytes_journal_observados) = {
        let lease = owner.journal_temporal().unwrap();
        let (id_bytes, bytes) = lease.abrir_y_leer_canonico().unwrap().unwrap();
        let (id_record, record) = lease.cargar_canonico().unwrap().unwrap();
        assert_eq!(id_bytes, id_record);
        (id_record, record, bytes)
    };
    assert_eq!(id_journal_cambiado, id_journal_reabierto);
    assert_eq!(bytes_journal_observados, bytes_journal_cambiado);
    let identidad_journal_antes_vinculo =
        identidad_archivo(&File::open(&ruta_journal).unwrap()).unwrap();
    let error = envelope_reabierto
        .validar_replay(
            &journal_cambiado,
            envelope_fixtures::MOTOR,
            envelope_fixtures::POLICY,
        )
        .unwrap_err();
    assert_eq!(error.codigo(), "admission_envelope_journal_mismatch");
    assert_eq!(fs::read(&ruta_contexto).unwrap(), bytes_contexto);
    assert_eq!(fs::read(&ruta_journal).unwrap(), bytes_journal_observados);
    assert_eq!(
        identidad_archivo(&File::open(&ruta_contexto).unwrap()).unwrap(),
        id_contexto_reabierto
    );
    assert_eq!(
        identidad_archivo(&File::open(&ruta_journal).unwrap()).unwrap(),
        identidad_journal_antes_vinculo
    );

    drop(owner);
    drop(lock);
    assert_eq!(fs::read(&ruta_contexto).unwrap(), bytes_contexto);
    assert_eq!(fs::read(&ruta_journal).unwrap(), bytes_journal_cambiado);
    assert_eq!(
        identidad_archivo(&File::open(&ruta_contexto).unwrap()).unwrap(),
        id_contexto_reabierto
    );
}

#[test]
fn solo_devuelve_ausencia_observada_con_root_presente_o_ausente() {
    for crear_root in [false, true] {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let (ubicacion, root) = ubicacion(&parent, crear_root);
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        let mut owner =
            super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
                .unwrap();
        let lease = owner.contexto_admision().unwrap();

        assert!(lease.leer_canonico().unwrap().is_none());
        assert_eq!(root.is_dir(), crear_root);
        assert!(!contexto_path(&ubicacion).exists());
    }
}

#[test]
fn rechaza_directorio_canonico_y_conserva_su_contenido() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let contexto = contexto_path(&ubicacion);
    fs::create_dir(&contexto).unwrap();
    fs::write(contexto.join("canary"), b"preserve directory").unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.contexto_admision().unwrap();

    let error = lease.leer_canonico().unwrap_err();

    assert!(matches!(
        error,
        ErrorContextoAdmisionAnclado::EntryRejected | ErrorContextoAdmisionAnclado::Io
    ));
    assert_eq!(
        fs::read(contexto.join("canary")).unwrap(),
        b"preserve directory"
    );
}

#[test]
fn lease_lee_el_parent_capturado_y_preserva_el_decoy_del_namespace() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let (ubicacion, root) = ubicacion(&parent, true);
    let actual = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let decoy = b"decoy context canary";
    fs::write(contexto_path(&ubicacion), &actual).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    if let Err(error) = fs::rename(&parent, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32)),
                "{error}"
            );
            assert!(root.is_dir());
            assert!(!moved_parent.exists());
            assert_eq!(fs::read(contexto_path(&ubicacion)).unwrap(), actual);
            eprintln!("Windows bloqueó el rename con el lock abierto; no se ejercitó el decoy.");
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent de prueba: {error}");
    }

    fs::create_dir(&parent).unwrap();
    fs::create_dir(parent.join("bundle")).unwrap();
    let decoy_context_path = contexto_path(&ubicacion);
    fs::write(&decoy_context_path, decoy).unwrap();
    let ruta_lock = ubicacion.lock();
    let lock_name = ruta_lock.as_path().file_name().unwrap().to_os_string();
    fs::write(parent.join(lock_name), b"decoy lock").unwrap();

    let lease = owner.contexto_admision().unwrap();
    let (envelope, identity) = lease.leer_canonico().unwrap().unwrap();

    assert_eq!(codec::codificar(&envelope).unwrap(), actual);
    assert_eq!(
        identity,
        identidad_archivo(
            &File::open(
                moved_parent.join(nombre_canonico(ubicacion.ruta().file_name().unwrap()).unwrap(),)
            )
            .unwrap()
        )
        .unwrap()
    );
    assert_eq!(fs::read(&decoy_context_path).unwrap().as_slice(), decoy);
    assert!(moved_parent.join("bundle").is_dir());
    assert!(root.exists());
}

#[test]
fn error_de_codec_no_se_convierte_en_ausencia_y_preserva_bytes_redactados() {
    let transaction_id = "0123456789abcdef0123456789abcdef";
    for truncar in [false, true] {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let (ubicacion, _) = ubicacion(&parent, true);
        let mut bytes = frame(transaction_id, parent_identity(&parent));
        if truncar {
            bytes.pop();
        } else {
            bytes[0] ^= 1;
        }
        let canonico = contexto_path(&ubicacion);
        fs::write(&canonico, &bytes).unwrap();
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        let mut owner =
            super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
                .unwrap();
        let lease = owner.contexto_admision().unwrap();
        let lease_debug = format!("{lease:?}");
        assert_eq!(lease_debug, "ContextoAdmisionAnclado { .. }");
        assert!(!lease_debug.contains(canonico.to_string_lossy().as_ref()));
        assert!(!lease_debug.contains(transaction_id));

        let error = lease.leer_canonico().unwrap_err();

        assert!(matches!(error, ErrorContextoAdmisionAnclado::Codec(_)));
        assert_eq!(error.codigo(), "admission_envelope_framing_invalid");
        assert_eq!(error.to_string(), error.codigo());
        let debug = format!("{error:?}");
        assert!(!debug.contains(transaction_id));
        assert!(!debug.contains(canonico.to_string_lossy().as_ref()));
        assert_eq!(fs::read(&canonico).unwrap(), bytes);
    }
}

#[cfg(unix)]
#[test]
fn symlink_canonico_no_se_sigue_y_preserva_el_target_y_el_enlace() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let target = temp.path().join("target-canary");
    let link = contexto_path(&ubicacion);
    fs::write(&target, b"outside target canary").unwrap();
    symlink(&target, &link).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.contexto_admision().unwrap();

    let error = lease.leer_canonico().unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::EntryRejected);
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_link(&link).unwrap(), target);
    assert_eq!(fs::read(&target).unwrap(), b"outside target canary");
}

#[test]
fn lease_de_contexto_rechaza_lock_sustituido_con_bytes_iguales() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let bytes_contexto = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let canonico = contexto_path(&ubicacion);
    fs::write(&canonico, &bytes_contexto).unwrap();
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let lock_bytes = b"same context lock bytes, different identity";
    fs::write(&lock_path, lock_bytes).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let identidad_guard = lock.identidad_archivo_lock().unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let desplazado = temp.path().join("original-lock-canary");

    if let Err(error) = fs::rename(&lock_path, &desplazado) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33)),
                "{error}"
            );
            assert!(lock_path.is_file());
            assert!(!desplazado.exists());
            drop(owner);
            drop(lock);
            let visible = File::open(&lock_path).unwrap();
            assert_eq!(identidad_archivo(&visible).unwrap(), identidad_guard);
            assert_eq!(
                fs::read(&lock_path).unwrap().as_slice(),
                lock_bytes.as_slice()
            );
            assert_eq!(fs::read(&canonico).unwrap(), bytes_contexto);
            eprintln!(
                "Windows bloqueó la sustitución del lock; no se ejercitó el rechazo por identidad."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo desplazar el lock de prueba: {error}");
    }

    fs::write(&lock_path, lock_bytes).unwrap();
    let visible = File::open(&lock_path).unwrap();
    assert_ne!(identidad_archivo(&visible).unwrap(), identidad_guard);

    let error = owner.contexto_admision().unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::OwnerUnverified);
    assert_eq!(error.codigo(), "admission_context_owner_unverified");
    drop(visible);
    drop(owner);
    drop(lock);
    assert_eq!(
        fs::read(&lock_path).unwrap().as_slice(),
        lock_bytes.as_slice()
    );
    assert_eq!(
        fs::read(&desplazado).unwrap().as_slice(),
        lock_bytes.as_slice()
    );
    assert_eq!(fs::read(&canonico).unwrap(), bytes_contexto);
}

#[test]
fn crea_temporal_exclusivo_y_devuelve_recibo_redactado_con_datos_del_archivo() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let esperado = codec::codificar(&envelope).unwrap();
    let temporal = temporal_contexto_path(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.contexto_admision().unwrap();

    let recibo = lease.crear_temporal(&envelope).unwrap();

    assert_eq!(fs::read(&temporal).unwrap(), esperado);
    assert!(!contexto_path(&ubicacion).exists());
    assert_eq!(recibo.datos.longitud, esperado.len());
    let digest_esperado: [u8; 32] = Sha256::digest(&esperado).into();
    assert_eq!(recibo.datos.digest, digest_esperado);
    let archivo = File::open(&temporal).unwrap();
    assert_eq!(
        recibo.datos.identidad_archivo,
        identidad_archivo(&archivo).unwrap()
    );
    let debug = format!("{recibo:?}");
    assert_eq!(debug, "ReciboContextoTemporal { .. }");
    let temporal_text = temporal.to_string_lossy();
    assert!(!debug.contains(temporal_text.as_ref()));
    assert!(!debug.contains("0123456789abcdef0123456789abcdef"));
}

#[test]
fn error_de_codificacion_no_crea_ni_modifica_el_temporal() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let temporal = temporal_contexto_path(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.contexto_admision().unwrap();

    let error = lease
        .crear_temporal_con(&envelope, |_| {
            assert!(!temporal.exists());
            Err(codec::ErrorCodecEnvelopeAdmision::ReservaFallida)
        })
        .unwrap_err();

    assert_eq!(
        error,
        ErrorContextoAdmisionAnclado::Codec(codec::ErrorCodecEnvelopeAdmision::ReservaFallida)
    );
    assert!(!temporal.exists());
    assert!(!contexto_path(&ubicacion).exists());
}

#[test]
fn rechaza_lock_sustituido_despues_del_lease_antes_de_crear_temporal() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let temporal = temporal_contexto_path(&ubicacion);
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let lock_bytes = b"same temporal lock bytes, different file identity";
    fs::write(&lock_path, lock_bytes).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let identidad_guard = lock.identidad_archivo_lock().unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.contexto_admision().unwrap();
    let desplazado = temp.path().join("temporal-original-lock-canary");

    if let Err(error) = fs::rename(&lock_path, &desplazado) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33)),
                "{error}"
            );
            assert!(lock_path.is_file());
            assert!(!desplazado.exists());
            drop(lease);
            drop(owner);
            drop(lock);
            let visible = File::open(&lock_path).unwrap();
            assert_eq!(identidad_archivo(&visible).unwrap(), identidad_guard);
            assert_eq!(
                fs::read(&lock_path).unwrap().as_slice(),
                lock_bytes.as_slice()
            );
            assert!(!temporal.exists());
            eprintln!(
                "Windows bloqueó la sustitución del lock; no se ejercitó el rechazo antes de crear el temporal."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo desplazar el lock de prueba: {error}");
    }

    fs::write(&lock_path, lock_bytes).unwrap();
    let visible = File::open(&lock_path).unwrap();
    assert_ne!(identidad_archivo(&visible).unwrap(), identidad_guard);

    let error = lease.crear_temporal(&envelope).unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::OwnerUnverified);
    assert!(!temporal.exists());
    drop(visible);
    drop(owner);
    drop(lock);
    assert_eq!(
        fs::read(&lock_path).unwrap().as_slice(),
        lock_bytes.as_slice()
    );
    assert_eq!(
        fs::read(&desplazado).unwrap().as_slice(),
        lock_bytes.as_slice()
    );
}

#[test]
fn crea_temporal_en_parent_capturado_y_preserva_namespace_decoy() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let (ubicacion, root) = ubicacion(&parent, true);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let esperado = codec::codificar(&envelope).unwrap();
    let temporal = temporal_contexto_path(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.contexto_admision().unwrap();

    if let Err(error) = fs::rename(&parent, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32)),
                "{error}"
            );
            assert!(root.is_dir());
            assert!(!moved_parent.exists());
            drop(lease);
            drop(owner);
            drop(lock);
            eprintln!(
                "Windows bloqueó el rename del parent bajo el lock; no se ejercitó el decoy."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent de prueba: {error}");
    }

    fs::create_dir(&parent).unwrap();
    fs::create_dir(parent.join("bundle")).unwrap();
    let decoy_temporal = temporal_contexto_path(&ubicacion);
    let temporal_canary = b"preserve decoy temporary";
    fs::write(&decoy_temporal, temporal_canary).unwrap();
    let ruta_lock = ubicacion.lock();
    let lock_name = ruta_lock.as_path().file_name().unwrap().to_os_string();
    let lock_canary = b"preserve decoy lock";
    let decoy_lock = parent.join(lock_name);
    fs::write(&decoy_lock, lock_canary).unwrap();

    let recibo = lease.crear_temporal(&envelope).unwrap();
    let temporal_real = moved_parent.join(temporal.file_name().unwrap());

    assert_eq!(fs::read(&temporal_real).unwrap(), esperado);
    assert_eq!(
        fs::read(&decoy_temporal).unwrap().as_slice(),
        temporal_canary.as_slice()
    );
    assert_eq!(
        fs::read(&decoy_lock).unwrap().as_slice(),
        lock_canary.as_slice()
    );
    assert_eq!(recibo.datos.longitud, esperado.len());
    assert_eq!(
        recibo.datos.identidad_archivo,
        identidad_archivo(&File::open(&temporal_real).unwrap()).unwrap()
    );
    assert!(root.exists());
}

#[test]
fn verifica_el_mismo_temporal_sin_publicarlo_ni_modificar_bytes() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, false);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let esperado = codec::codificar(&envelope).unwrap();
    let temporal = temporal_contexto_path(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let recibo = crear_recibo(&mut owner, &envelope);

    let verificado = recibo.verificar().unwrap();
    assert_eq!(
        format!("{verificado:?}"),
        "ContextoTemporalVerificado { .. }"
    );
    drop(verificado);

    assert_eq!(fs::read(&temporal).unwrap(), esperado);
    assert!(!contexto_path(&ubicacion).exists());
}

#[test]
fn rechaza_temporal_sustituido_por_otro_id_y_preserva_ambos_archivos() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, false);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let esperado = codec::codificar(&envelope).unwrap();
    let temporal = temporal_contexto_path(&ubicacion);
    let original = temp.path().join("original-temporal-canary");
    let canary = b"replacement temporary canary";
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let recibo = crear_recibo(&mut owner, &envelope);

    fs::rename(&temporal, &original).unwrap();
    fs::write(&temporal, canary).unwrap();
    let error = recibo.verificar().unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::TemporaryUnverified);
    assert_eq!(fs::read(&temporal).unwrap().as_slice(), canary.as_slice());
    assert_eq!(fs::read(&original).unwrap(), esperado);
}

#[test]
fn rechaza_temporal_mutado_y_truncado_sin_alterar_el_archivo_observado() {
    for truncar in [false, true] {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let (ubicacion, _) = ubicacion(&parent, false);
        let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
        let temporal = temporal_contexto_path(&ubicacion);
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        let mut owner =
            super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
                .unwrap();
        let recibo = crear_recibo(&mut owner, &envelope);

        if truncar {
            OpenOptions::new()
                .write(true)
                .open(&temporal)
                .unwrap()
                .set_len(0)
                .unwrap();
        } else {
            let mut bytes = fs::read(&temporal).unwrap();
            bytes[0] ^= 1;
            fs::write(&temporal, &bytes).unwrap();
        }
        let bytes_observados = fs::read(&temporal).unwrap();
        let error = recibo.verificar().unwrap_err();

        assert_eq!(error, ErrorContextoAdmisionAnclado::TemporaryUnverified);
        assert_eq!(fs::read(&temporal).unwrap(), bytes_observados);
    }
}

#[test]
fn verificacion_revalida_lock_y_no_abre_el_temporal_si_fue_sustituido() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, false);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let temporal = temporal_contexto_path(&ubicacion);
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let lock_bytes = b"same lock bytes, another identity";
    fs::write(&lock_path, lock_bytes).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let identidad_guard = lock.identidad_archivo_lock().unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let recibo = crear_recibo(&mut owner, &envelope);
    let desplazado = temp.path().join("original-lock-canary");

    if let Err(error) = fs::rename(&lock_path, &desplazado) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33)),
                "{error}"
            );
            assert!(lock_path.is_file());
            assert!(!desplazado.exists());
            drop(recibo);
            drop(owner);
            drop(lock);
            assert_eq!(
                fs::read(&temporal).unwrap(),
                codec::codificar(&envelope).unwrap()
            );
            eprintln!(
                "Windows bloqueó la sustitución del lock; no se ejercitó el rechazo de verificación."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo desplazar el lock de prueba: {error}");
    }

    fs::write(&lock_path, lock_bytes).unwrap();
    let visible = File::open(&lock_path).unwrap();
    assert_ne!(identidad_archivo(&visible).unwrap(), identidad_guard);
    let error = recibo.verificar().unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::OwnerUnverified);
    assert_eq!(
        fs::read(&temporal).unwrap(),
        codec::codificar(&envelope).unwrap()
    );
    drop(visible);
    drop(owner);
    drop(lock);
    assert_eq!(
        fs::read(&lock_path).unwrap().as_slice(),
        lock_bytes.as_slice()
    );
    assert_eq!(
        fs::read(&desplazado).unwrap().as_slice(),
        lock_bytes.as_slice()
    );
}

#[test]
fn verifica_desde_parent_capturado_y_preserva_temporal_decoy() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let (ubicacion, _) = ubicacion(&parent, false);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let esperado = codec::codificar(&envelope).unwrap();
    let temporal = temporal_contexto_path(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let recibo = crear_recibo(&mut owner, &envelope);

    if let Err(error) = fs::rename(&parent, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32)),
                "{error}"
            );
            assert!(parent.is_dir());
            assert!(!moved_parent.exists());
            drop(recibo);
            drop(owner);
            drop(lock);
            eprintln!("Windows bloqueó el rename del parent; no se ejercitó el decoy.");
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent de prueba: {error}");
    }

    fs::create_dir(&parent).unwrap();
    fs::create_dir(parent.join("bundle")).unwrap();
    let decoy_temporal = temporal_contexto_path(&ubicacion);
    let canary = b"preserve decoy temporary";
    fs::write(&decoy_temporal, canary).unwrap();

    let verificado = recibo.verificar().unwrap();
    drop(verificado);

    let temporal_real = moved_parent.join(temporal.file_name().unwrap());
    assert_eq!(fs::read(&temporal_real).unwrap(), esperado);
    assert_eq!(
        fs::read(&decoy_temporal).unwrap().as_slice(),
        canary.as_slice()
    );
    assert!(!contexto_path(&ubicacion).exists());
}

#[test]
fn publica_contexto_y_libera_el_lock_para_el_lease_secuencial_del_journal() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let esperado = codec::codificar(&envelope).unwrap();
    let temporal = temporal_contexto_path(&ubicacion);
    let canonico = contexto_path(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let verificado = crear_recibo(&mut owner, &envelope).verificar().unwrap();

    let publicado = verificado.publicar().unwrap();

    assert_eq!(fs::read(&canonico).unwrap(), esperado);
    assert!(!temporal.exists());
    assert_eq!(format!("{publicado:?}"), "ReciboContextoPublicado { .. }");
    let journal = owner.journal_temporal().unwrap();
    drop(journal);
}

#[test]
fn publicar_usa_el_parent_capturado_y_preserva_decoys_del_namespace() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let (ubicacion, root) = ubicacion(&parent, true);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let esperado = codec::codificar(&envelope).unwrap();
    let temporal = temporal_contexto_path(&ubicacion);
    let canonico = contexto_path(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let verificado = crear_recibo(&mut owner, &envelope).verificar().unwrap();

    if let Err(error) = fs::rename(&parent, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33)),
                "{error}"
            );
            assert!(root.is_dir());
            assert!(!moved_parent.exists());
            drop(verificado);
            drop(owner);
            drop(lock);
            assert!(temporal.is_file());
            assert!(!canonico.exists());
            eprintln!("Windows bloqueó el rename del parent; no se ejercitó el decoy.");
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent de prueba: {error}");
    }

    fs::create_dir(&parent).unwrap();
    fs::create_dir(parent.join("bundle")).unwrap();
    let decoy_temporal = temporal_contexto_path(&ubicacion);
    let decoy_canonico = contexto_path(&ubicacion);
    let temporal_canary = b"preserve decoy temporary";
    let canonico_canary = b"preserve decoy context";
    fs::write(&decoy_temporal, temporal_canary).unwrap();
    fs::write(&decoy_canonico, canonico_canary).unwrap();

    let publicado = verificado.publicar().unwrap();

    let nombre_temporal = temporal.file_name().unwrap();
    let nombre_canonico = canonico.file_name().unwrap();
    assert_eq!(
        fs::read(moved_parent.join(nombre_canonico)).unwrap(),
        esperado
    );
    assert!(!moved_parent.join(nombre_temporal).exists());
    assert_eq!(fs::read(&decoy_temporal).unwrap(), temporal_canary);
    assert_eq!(fs::read(&decoy_canonico).unwrap(), canonico_canary);
    assert!(root.exists());
    assert_eq!(format!("{publicado:?}"), "ReciboContextoPublicado { .. }");
}

#[test]
fn canonic_conflictivo_no_reemplaza_ni_limpia_el_temporal() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let esperado = codec::codificar(&envelope).unwrap();
    let canary = b"preserve canonical canary";
    let canonico = contexto_path(&ubicacion);
    let temporal = temporal_contexto_path(&ubicacion);
    fs::write(&canonico, canary).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let verificado = crear_recibo(&mut owner, &envelope).verificar().unwrap();

    let error = verificado.publicar().unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::PublicationUnverified);
    assert_eq!(error.codigo(), "admission_context_publication_unverified");
    assert_eq!(fs::read(&canonico).unwrap(), canary);
    assert_eq!(fs::read(&temporal).unwrap(), esperado);
}

#[test]
fn publicar_rechaza_basename_temporal_sustituido_despues_de_verificar() {
    use std::cell::Cell;

    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved = temp.path().join("original-temporal");
    let (ubicacion, _) = ubicacion(&parent, true);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let esperado = codec::codificar(&envelope).unwrap();
    let temporal = temporal_contexto_path(&ubicacion);
    let canonico = contexto_path(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let verificado = crear_recibo(&mut owner, &envelope).verificar().unwrap();
    let identidad_original = verificado.datos.identidad_archivo;

    if let Err(error) = fs::rename(&temporal, &moved) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33)),
                "{error}"
            );
            drop(verificado);
            drop(owner);
            drop(lock);
            assert_eq!(fs::read(&temporal).unwrap(), esperado);
            assert!(!moved.exists());
            eprintln!(
                "Windows bloqueó sustituir el temporal abierto; no se ejercitó el rechazo por ID."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo desplazar el temporal de prueba: {error}");
    }
    fs::write(&temporal, &esperado).unwrap();
    let decoy = File::open(&temporal).unwrap();
    assert_ne!(identidad_archivo(&decoy).unwrap(), identidad_original);
    drop(decoy);
    let llamadas = Cell::new(0);

    let error = verificado
        .publicar_con(|_, _, _, _| {
            llamadas.set(llamadas.get() + 1);
            Ok(())
        })
        .unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::TemporaryUnverified);
    assert_eq!(llamadas.get(), 0);
    drop(owner);
    drop(lock);
    assert_eq!(fs::read(&moved).unwrap(), esperado);
    assert_eq!(fs::read(&temporal).unwrap(), esperado);
    assert!(!canonico.exists());
}

#[test]
fn confirma_rename_aplicado_aunque_el_syscall_devuelva_error_y_no_reintenta() {
    use std::cell::Cell;

    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let envelope = envelope("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let esperado = codec::codificar(&envelope).unwrap();
    let temporal = temporal_contexto_path(&ubicacion);
    let canonico = contexto_path(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let verificado = crear_recibo(&mut owner, &envelope).verificar().unwrap();
    let llamadas = Cell::new(0);

    let publicado = verificado
        .publicar_con(|parent, fuente, nombre_temporal, nombre_canonico| {
            llamadas.set(llamadas.get() + 1);
            super::publicar::renombrar_no_reemplazar(
                parent,
                fuente,
                nombre_temporal,
                nombre_canonico,
            )?;
            Err(())
        })
        .unwrap();

    assert_eq!(llamadas.get(), 1);
    assert_eq!(fs::read(&canonico).unwrap(), esperado);
    assert!(!temporal.exists());
    assert_eq!(format!("{publicado:?}"), "ReciboContextoPublicado { .. }");
}

#[test]
fn evidencia_de_lectura_canonica_resume_bytes_y_handle_observados() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let esperado = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let canonico = contexto_path(&ubicacion);
    fs::write(&canonico, &esperado).unwrap();
    let identidad_esperada = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.contexto_admision().unwrap();

    let (_envelope, evidencia) = lease.leer_canonico_con_evidencia().unwrap().unwrap();

    let digest_esperado: [u8; 32] = Sha256::digest(&esperado).into();
    assert_eq!(evidencia.identidad_archivo, identidad_esperada);
    assert_eq!(evidencia.longitud, u64::try_from(esperado.len()).unwrap());
    assert_eq!(evidencia.sha256, digest_esperado);
    assert_eq!(fs::read(&canonico).unwrap(), esperado);
    assert_eq!(format!("{evidencia:?}"), "EvidenciaLecturaContexto { .. }");
}

#[test]
fn evidencia_cambia_si_muta_el_contenido_in_place_con_el_mismo_id() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let identidad_parent = parent_identity(&parent);
    let primero = frame("0123456789abcdef0123456789abcdef", identidad_parent.clone());
    let segundo = frame("1123456789abcdef0123456789abcdef", identidad_parent);
    let canonico = contexto_path(&ubicacion);
    fs::write(&canonico, &primero).unwrap();
    let identidad_antes = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidencia_primera) = lease.leer_canonico_con_evidencia().unwrap().unwrap();

    fs::write(&canonico, &segundo).unwrap();
    let identidad_despues = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();
    let (_, evidencia_segunda) = lease.leer_canonico_con_evidencia().unwrap().unwrap();

    let digest_primero: [u8; 32] = Sha256::digest(&primero).into();
    let digest_segundo: [u8; 32] = Sha256::digest(&segundo).into();
    assert_eq!(identidad_despues, identidad_antes);
    assert_eq!(evidencia_primera.identidad_archivo, identidad_antes);
    assert_eq!(evidencia_segunda.identidad_archivo, identidad_antes);
    assert_eq!(evidencia_primera.sha256, digest_primero);
    assert_eq!(evidencia_segunda.sha256, digest_segundo);
    assert_ne!(evidencia_primera.sha256, evidencia_segunda.sha256);
    assert_eq!(
        evidencia_segunda.longitud,
        u64::try_from(segundo.len()).unwrap()
    );
    assert_eq!(fs::read(&canonico).unwrap(), segundo);
}

#[test]
fn error_de_lectura_con_evidencia_preserva_bytes_y_no_devuelve_evidencia() {
    for trailing in [false, true] {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let (ubicacion, _) = ubicacion(&parent, true);
        let mut bytes = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
        if trailing {
            bytes.push(0);
        } else {
            bytes[0] ^= 1;
        }
        let canonico = contexto_path(&ubicacion);
        fs::write(&canonico, &bytes).unwrap();
        let identidad_antes = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        let mut owner =
            super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
                .unwrap();
        let lease = owner.contexto_admision().unwrap();

        let error = lease.leer_canonico_con_evidencia().unwrap_err();

        assert!(matches!(error, ErrorContextoAdmisionAnclado::Codec(_)));
        assert_eq!(error.codigo(), "admission_envelope_framing_invalid");
        assert_eq!(fs::read(&canonico).unwrap(), bytes);
        assert_eq!(
            identidad_archivo(&File::open(&canonico).unwrap()).unwrap(),
            identidad_antes
        );
    }
}
