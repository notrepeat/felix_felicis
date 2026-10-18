use super::*;
use crate::transaccion::anclado::IdentidadArchivo;
use crate::transaccion::anclado::contexto_admision::ReciboContextoPublicado;
use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::anclado::duenio_transaccion::context_journal_fixture_tests::{
    FixtureContextoJournalV2, preparar_fixture_contexto_journal_v2,
};
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    id_archivo, verificar_arbol,
};
use crate::transaccion::anclado::duenio_transaccion::normal_v2_test_support::verificar_canary;
use crate::transaccion::lock::LockEscritura;
use std::fs;
use tempfile::tempdir;

const LOCK_CANARY: &[u8] = b"context-journal-fixture-lock-canary";

fn publicar_contexto(
    owner: &mut DuenioTransaccionAnclado<'_>,
    fixture: &FixtureContextoJournalV2,
) -> ReciboContextoPublicado {
    owner
        .contexto_admision()
        .unwrap()
        .crear_temporal_v2(&fixture.envelope)
        .unwrap()
        .verificar()
        .unwrap()
        .publicar()
        .unwrap()
}

fn revisar_canarios(fixture: &FixtureContextoJournalV2, lock_id: IdentidadArchivo) {
    assert_eq!(fs::read(&fixture.lock).unwrap(), LOCK_CANARY);
    assert_eq!(id_archivo(&fixture.lock), lock_id);
    verificar_arbol(&fixture.fixture.root, &fixture.fixture.entradas);
    verificar_canary(&fixture.canary, fixture.identidad_canary);
}

#[test]
fn observador_contexto_v2_valida_recibo_binding_y_replay() {
    let temp = tempdir().unwrap();
    let fixture = preparar_fixture_contexto_journal_v2(&temp.path().join("parent"));
    let lock_id;
    let contexto_id;

    {
        let mut lock = LockEscritura::adquirir_en(&fixture.fixture.ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let recibo = publicar_contexto(&mut owner, &fixture);
        contexto_id = id_archivo(&fixture.contexto);

        let resultado = observar_contexto_con_journal_v2(&mut owner, &recibo, &fixture.journal);
        let EstadoObservacionContextoJournalV2::Observado(observacion) = &resultado else {
            panic!("el recibo, el binding y el replay deben coincidir");
        };
        assert_eq!(observacion.evidencia_raw_retiro().0, contexto_id);
        assert!(matches!(
            observacion.envelope_versionado(),
            EnvelopeAdmisionVersionada::V2(_)
        ));
        drop(resultado);
    }

    assert_eq!(
        fs::read(&fixture.contexto).unwrap(),
        fixture.fixture.bytes_contexto
    );
    assert_eq!(id_archivo(&fixture.contexto), contexto_id);
    assert!(!fixture.journal_canonico.exists());
    revisar_canarios(&fixture, lock_id);
}

#[test]
fn observador_contexto_v2_reporta_ausencia_sin_publicar() {
    let temp = tempdir().unwrap();
    let fixture = preparar_fixture_contexto_journal_v2(&temp.path().join("parent"));
    let lock_id;

    {
        let mut lock = LockEscritura::adquirir_en(&fixture.fixture.ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let recibo = publicar_contexto(&mut owner, &fixture);
        fs::remove_file(&fixture.contexto).unwrap();

        let resultado = observar_contexto_con_journal_v2(&mut owner, &recibo, &fixture.journal);
        assert!(matches!(
            resultado,
            EstadoObservacionContextoJournalV2::Ausente
        ));
        assert!(!fixture.journal_canonico.exists());
    }

    revisar_canarios(&fixture, lock_id);
}

#[test]
fn observador_contexto_v2_rechaza_mismos_bytes_con_identidad_distinta() {
    let temp = tempdir().unwrap();
    let fixture = preparar_fixture_contexto_journal_v2(&temp.path().join("parent"));
    let decoy = fixture.contexto.with_extension("contexto-decoy");
    let lock_id;
    let identidad_original;
    let identidad_reemplazo;

    {
        let mut lock = LockEscritura::adquirir_en(&fixture.fixture.ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let recibo = publicar_contexto(&mut owner, &fixture);
        let original = observar_contexto_con_journal_v2(&mut owner, &recibo, &fixture.journal);
        let EstadoObservacionContextoJournalV2::Observado(observacion_original) = &original else {
            panic!("el Contexto publicado debe observarse antes del reemplazo");
        };
        identidad_original = observacion_original.evidencia_raw_retiro().0;

        fs::write(&decoy, &fixture.fixture.bytes_contexto).unwrap();
        let identidad_decoy = id_archivo(&decoy);
        fs::remove_file(&fixture.contexto).unwrap();
        fs::rename(&decoy, &fixture.contexto).unwrap();
        assert_ne!(identidad_decoy, identidad_original);
        assert_eq!(
            fs::read(&fixture.contexto).unwrap(),
            fixture.fixture.bytes_contexto
        );
        assert_eq!(id_archivo(&fixture.contexto), identidad_decoy);
        identidad_reemplazo = identidad_decoy;

        let reemplazo = observar_contexto_con_journal_v2(&mut owner, &recibo, &fixture.journal);
        let EstadoObservacionContextoJournalV2::Rechazado {
            observacion,
            error: ErrorObservacionContextoJournalV2::ReciboNoCoincide,
        } = &reemplazo
        else {
            panic!("los bytes iguales no sustituyen la identidad del recibo");
        };
        assert_eq!(observacion.evidencia_raw_retiro().0, identidad_decoy);
        drop(reemplazo);
        drop(original);
    }

    assert_eq!(id_archivo(&fixture.contexto), identidad_reemplazo);
    revisar_canarios(&fixture, lock_id);
}
