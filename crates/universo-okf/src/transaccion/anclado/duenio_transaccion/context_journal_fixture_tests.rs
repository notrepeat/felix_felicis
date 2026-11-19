use super::*;
use crate::transaccion::admision::envelope::selector::{
    EnvelopeAdmisionVersionada, decodificar as decodificar_envelope,
};
use crate::transaccion::anclado::contexto_admision::ReciboContextoPublicado;
use crate::transaccion::anclado::duenio_transaccion::ReciboTemporalJournalV2;
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    FixturePublicada, contexto_path, id_archivo, verificar_arbol,
};
use crate::transaccion::anclado::duenio_transaccion::normal_v2_test_support::{
    crear_canary, preparar_normal_v2, verificar_canary,
};
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::journal::{ArtifactFingerprint, JournalState};
use crate::transaccion::lock::LockEscritura;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

const LOCK_CANARY: &[u8] = b"context-journal-fixture-lock-canary";

pub(super) struct FixtureContextoJournalV2 {
    pub(super) fixture: FixturePublicada,
    pub(super) journal: JournalV2,
    pub(super) envelope: crate::transaccion::admision::envelope::v2::EnvelopeContextoAdmisionV2,
    pub(super) contexto: PathBuf,
    pub(super) journal_canonico: PathBuf,
    pub(super) lock: PathBuf,
    pub(super) canary: PathBuf,
    pub(super) identidad_canary: crate::transaccion::anclado::IdentidadArchivo,
}

pub(super) fn preparar_fixture_contexto_journal_v2(parent: &Path) -> FixtureContextoJournalV2 {
    let (fixture, digest) = preparar_normal_v2(parent, JournalState::Prepared);
    assert!(digest.is_some());
    let journal = JournalV2::parse(&fixture.bytes_journal).unwrap();
    assert!(journal.state() == JournalState::Prepared);
    let envelope = match decodificar_envelope(&fixture.bytes_contexto).unwrap() {
        EnvelopeAdmisionVersionada::V2(envelope) => envelope,
        EnvelopeAdmisionVersionada::V1(_) => panic!("el fixture debe ser V2"),
    };
    let contexto = contexto_path(&fixture.ubicacion);
    let journal_canonico = fixture.ubicacion.journal().as_path().to_path_buf();
    fs::remove_file(&contexto).unwrap();
    fs::remove_file(&journal_canonico).unwrap();
    let (canary, identidad_canary) = crear_canary(&fixture);
    let lock = fixture.ubicacion.lock().as_path().to_path_buf();
    fs::write(&lock, LOCK_CANARY).unwrap();

    FixtureContextoJournalV2 {
        fixture,
        journal,
        envelope,
        contexto,
        journal_canonico,
        lock,
        canary,
        identidad_canary,
    }
}

#[test]
fn fixture_contexto_v2_publicado_conserva_binding_replay_y_siblings() {
    let temp = tempdir().unwrap();
    let fixture = preparar_fixture_contexto_journal_v2(&temp.path().join("parent"));
    assert!(!fixture.contexto.exists());
    assert!(!fixture.journal_canonico.exists());
    let identidad_contexto;
    let identidad_lock;

    {
        let mut lock = LockEscritura::adquirir_en(&fixture.fixture.ubicacion).unwrap();
        identidad_lock = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let recibo = owner
            .contexto_admision()
            .unwrap()
            .crear_temporal_v2(&fixture.envelope)
            .unwrap()
            .verificar()
            .unwrap()
            .publicar()
            .unwrap();
        identidad_contexto = id_archivo(&fixture.contexto);
        assert_eq!(
            fs::read(&fixture.contexto).unwrap(),
            fixture.fixture.bytes_contexto
        );
        assert!(!fixture.journal_canonico.exists());

        let lease = owner.contexto_admision().unwrap();
        let observacion = lease.observar_versionado().unwrap().unwrap();
        assert_eq!(
            recibo.validar_observacion_v2(&lease, &observacion, &fixture.journal),
            Ok(())
        );
        assert_eq!(observacion.evidencia_raw_retiro().0, identidad_contexto);
        let EnvelopeAdmisionVersionada::V2(observado) = observacion.envelope_versionado() else {
            panic!("el frame observado debe conservar V2");
        };
        let replay = observado.calcular_new_esperado(&fixture.journal).unwrap();
        let esperado = ArtifactFingerprint::new(
            replay.journal().new_fingerprint().identity().clone(),
            replay.digest().clone(),
        );
        assert!(esperado == *fixture.journal.new_fingerprint());
    }

    assert_eq!(
        fs::read(&fixture.contexto).unwrap(),
        fixture.fixture.bytes_contexto
    );
    assert_eq!(id_archivo(&fixture.contexto), identidad_contexto);
    assert!(!fixture.journal_canonico.exists());
    assert_eq!(fs::read(&fixture.lock).unwrap(), LOCK_CANARY);
    assert_eq!(id_archivo(&fixture.lock), identidad_lock);
    verificar_arbol(&fixture.fixture.root, &fixture.fixture.entradas);
    verificar_canary(&fixture.canary, fixture.identidad_canary);
}

pub(super) fn preparar_recibos<'owner, 'lock>(
    owner: &'owner mut DuenioTransaccionAnclado<'lock>,
    fixture: &FixtureContextoJournalV2,
) -> (
    ReciboContextoPublicado,
    ReciboTemporalJournalV2<'owner, 'lock>,
) {
    let recibo_contexto = owner
        .contexto_admision()
        .unwrap()
        .crear_temporal_v2(&fixture.envelope)
        .unwrap()
        .verificar()
        .unwrap()
        .publicar()
        .unwrap();
    let recibo_journal = owner.crear_temporal_journal_v2(&fixture.journal).unwrap();
    (recibo_contexto, recibo_journal)
}

pub(super) fn journal_temporal(fixture: &FixtureContextoJournalV2) -> PathBuf {
    let root = fixture.fixture.ubicacion.ruta();
    let mut nombre = std::ffi::OsString::from(".");
    nombre.push(root.file_name().unwrap());
    nombre.push(".felix.journal.tmp");
    root.parent().unwrap().join(nombre)
}

pub(super) fn revisar_canarios_y_arbol(
    fixture: &FixtureContextoJournalV2,
    lock_id: crate::transaccion::anclado::IdentidadArchivo,
) {
    assert_eq!(fs::read(&fixture.lock).unwrap(), LOCK_CANARY);
    assert_eq!(id_archivo(&fixture.lock), lock_id);
    verificar_arbol(&fixture.fixture.root, &fixture.fixture.entradas);
    verificar_canary(&fixture.canary, fixture.identidad_canary);
}

#[test]
fn fixture_journal_temporal_crea_recibos_reales_y_preserva_siblings() {
    let temp = tempdir().unwrap();
    let fixture = preparar_fixture_contexto_journal_v2(&temp.path().join("parent"));
    let temporal = journal_temporal(&fixture);
    let bytes_journal = fixture.journal.encode().unwrap();
    let lock_id;
    let contexto_id;
    let temporal_id;

    {
        let mut lock = LockEscritura::adquirir_en(&fixture.fixture.ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (_recibo_contexto, _recibo_journal) = preparar_recibos(&mut owner, &fixture);
        contexto_id = id_archivo(&fixture.contexto);
        temporal_id = id_archivo(&temporal);
        assert_eq!(
            fs::read(&fixture.contexto).unwrap(),
            fixture.fixture.bytes_contexto
        );
        assert_eq!(fs::read(&temporal).unwrap(), bytes_journal);
        assert!(!fixture.journal_canonico.exists());
    }

    assert_eq!(
        fs::read(&fixture.contexto).unwrap(),
        fixture.fixture.bytes_contexto
    );
    assert_eq!(id_archivo(&fixture.contexto), contexto_id);
    assert_eq!(fs::read(&temporal).unwrap(), bytes_journal);
    assert_eq!(id_archivo(&temporal), temporal_id);
    assert!(!fixture.journal_canonico.exists());
    revisar_canarios_y_arbol(&fixture, lock_id);
}
