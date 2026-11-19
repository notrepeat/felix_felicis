use super::*;
use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::anclado::duenio_transaccion::context_journal_fixture_tests::{
    journal_temporal, preparar_fixture_contexto_journal_v2, preparar_recibos,
    revisar_canarios_y_arbol,
};
use crate::transaccion::anclado::duenio_transaccion::context_journal_v2_observation::{
    ErrorObservacionContextoJournalV2 as ErrorContinuidadContextoJournalV2,
    EstadoObservacionContextoJournalV2 as EstadoContextoJournalV2,
};
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::id_archivo;
use crate::transaccion::anclado::journal_temporal::{
    ErrorJournalAnclado, ResultadoPublicacionJournal,
};
use crate::transaccion::journal::JournalState;
use crate::transaccion::lock::LockEscritura;
use std::fs;
use tempfile::tempdir;

#[test]
fn confirmado_real_contexto_corrupto_y_post_io_conservan_observaciones() {
    let temp = tempdir().unwrap();
    let fixture = preparar_fixture_contexto_journal_v2(&temp.path().join("parent"));
    let temporal = journal_temporal(&fixture);
    let bytes_journal = fixture.journal.encode().unwrap();
    let bytes_contexto_corrupto = b"invalid context frame".to_vec();
    let lock_id;
    let id_temporal;

    {
        let mut lock = LockEscritura::adquirir_en(&fixture.fixture.ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (recibo_contexto, recibo_journal) = preparar_recibos(&mut owner, &fixture);
        id_temporal = id_archivo(&temporal);

        let resultado = recibo_journal.publicar_con_contexto_con_prueba(
            &recibo_contexto,
            |owner, identidad, journal| {
                let lease = owner.journal_temporal_estricto()?;
                let publicacion = lease.publicar_v2(identidad, journal);
                assert!(matches!(
                    &publicacion,
                    ResultadoPublicacionJournal::Confirmado { .. }
                ));
                fs::write(&fixture.contexto, &bytes_contexto_corrupto)
                    .map_err(|_| ErrorJournalAnclado::Io)?;
                Ok(publicacion)
            },
            |_, cuerpo| {
                assert!(matches!(
                    &cuerpo.inicial,
                    EstadoContextoJournalV2::Observado(_)
                ));
                assert!(matches!(
                    &cuerpo.publicacion,
                    Some(Ok(ResultadoPublicacionJournal::Confirmado { .. }))
                ));
                assert!(matches!(
                    &cuerpo.final_contexto,
                    Some(EstadoContextoJournalV2::Error(
                        ErrorContinuidadContextoJournalV2::LecturaContexto
                    ))
                ));
                Err(ErrorJournalAnclado::Io)
            },
        );

        let ResultadoOwnerPostcheck::PostcheckNoVerificado { cuerpo, error } = resultado else {
            panic!("el postcheck exterior debe dominar la publicación confirmada");
        };
        assert!(matches!(error, ErrorJournalAnclado::Io));
        assert!(matches!(
            &cuerpo.inicial,
            EstadoContextoJournalV2::Observado(_)
        ));
        assert!(matches!(
            &cuerpo.publicacion,
            Some(Ok(ResultadoPublicacionJournal::Confirmado { .. }))
        ));
        assert!(matches!(
            &cuerpo.final_contexto,
            Some(EstadoContextoJournalV2::Error(
                ErrorContinuidadContextoJournalV2::LecturaContexto
            ))
        ));
        assert_eq!(
            fs::read(&fixture.contexto).unwrap(),
            bytes_contexto_corrupto
        );
        assert_eq!(fs::read(&fixture.journal_canonico).unwrap(), bytes_journal);
        assert_eq!(id_archivo(&fixture.journal_canonico), id_temporal);
        assert!(!temporal.exists());
    }

    revisar_canarios_y_arbol(&fixture, lock_id);
}

#[test]
fn recibo_journal_no_preparado_no_publica_y_conserva_temporal_real() {
    let temp = tempdir().unwrap();
    let fixture = preparar_fixture_contexto_journal_v2(&temp.path().join("parent"));
    let temporal = journal_temporal(&fixture);
    let mut journal = fixture.journal.clone();
    journal.advance(JournalState::BackupReady).unwrap();
    let bytes_journal = journal.encode().unwrap();
    let lock_id;
    let id_temporal;
    let publisher_invocado = std::cell::Cell::new(false);
    let postcheck_invocado = std::cell::Cell::new(false);

    {
        let mut lock = LockEscritura::adquirir_en(&fixture.fixture.ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let recibo_contexto = owner
            .contexto_admision()
            .unwrap()
            .crear_temporal_v2(&fixture.envelope)
            .unwrap()
            .verificar()
            .unwrap()
            .publicar()
            .unwrap();
        let recibo_journal = owner.crear_temporal_journal_v2(&journal).unwrap();
        id_temporal = id_archivo(&temporal);

        let resultado = recibo_journal.publicar_con_contexto_con_prueba(
            &recibo_contexto,
            |_, _, _| {
                publisher_invocado.set(true);
                Err(ErrorJournalAnclado::Io)
            },
            |owner, _| {
                postcheck_invocado.set(true);
                owner.parent_revalidado().map(|_| ())
            },
        );

        let ResultadoOwnerPostcheck::PostcheckVerificado { cuerpo } = resultado else {
            panic!("el postcheck del owner debe cerrar la observación");
        };
        assert!(!publisher_invocado.get());
        assert!(postcheck_invocado.get());
        assert!(matches!(
            cuerpo.error_fase,
            Some(ErrorFaseJournalV2::NoPreparado)
        ));
        assert!(cuerpo.publicacion.is_none());
        assert!(cuerpo.final_contexto.is_none());
        assert!(!fixture.journal_canonico.exists());
        assert_eq!(fs::read(&temporal).unwrap(), bytes_journal);
        assert_eq!(id_archivo(&temporal), id_temporal);
    }

    revisar_canarios_y_arbol(&fixture, lock_id);
}
