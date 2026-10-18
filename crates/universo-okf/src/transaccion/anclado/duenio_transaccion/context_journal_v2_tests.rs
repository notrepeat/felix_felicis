use super::*;
use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::anclado::duenio_transaccion::context_journal_fixture_tests::{
    journal_temporal, preparar_fixture_contexto_journal_v2, preparar_recibos,
    revisar_canarios_y_arbol,
};
use crate::transaccion::anclado::duenio_transaccion::context_journal_v2_observation::ErrorObservacionContextoJournalV2 as ErrorContinuidadContextoJournalV2;
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::id_archivo;
use crate::transaccion::anclado::journal_temporal::{
    ErrorJournalAnclado, ResultadoPublicacionJournal,
};
use crate::transaccion::lock::LockEscritura;
use std::fs;
use tempfile::tempdir;

#[test]
fn recibo_journal_publica_con_contexto_y_conserva_observaciones_hasta_postcheck() {
    let temp = tempdir().unwrap();
    let fixture = preparar_fixture_contexto_journal_v2(&temp.path().join("parent"));
    let temporal = journal_temporal(&fixture);
    let bytes_journal = fixture.journal.encode().unwrap();
    let lock_id;
    let id_contexto;
    let id_journal;

    {
        let mut lock = LockEscritura::adquirir_en(&fixture.fixture.ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (recibo_contexto, recibo_journal) = preparar_recibos(&mut owner, &fixture);
        id_contexto = id_archivo(&fixture.contexto);
        id_journal = id_archivo(&temporal);

        let resultado = recibo_journal.publicar_con_contexto(&recibo_contexto);
        let ResultadoOwnerPostcheck::PostcheckVerificado { cuerpo } = resultado else {
            panic!("el postcheck del owner debe completar la observación");
        };
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
            Some(EstadoContextoJournalV2::Observado(_))
        ));
        assert!(cuerpo.error_fase.is_none());
        assert_eq!(
            fs::read(&fixture.contexto).unwrap(),
            fixture.fixture.bytes_contexto
        );
        assert_eq!(id_archivo(&fixture.contexto), id_contexto);
        assert_eq!(fs::read(&fixture.journal_canonico).unwrap(), bytes_journal);
        assert_eq!(id_archivo(&fixture.journal_canonico), id_journal);
        assert!(!temporal.exists());
    }

    revisar_canarios_y_arbol(&fixture, lock_id);
}

#[test]
fn contexto_ausente_antes_de_publicar_no_publica_journal_y_postcheck_sigue() {
    let temp = tempdir().unwrap();
    let fixture = preparar_fixture_contexto_journal_v2(&temp.path().join("parent"));
    let temporal = journal_temporal(&fixture);
    let lock_id;
    let temporal_id;

    {
        let mut lock = LockEscritura::adquirir_en(&fixture.fixture.ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (recibo_contexto, recibo_journal) = preparar_recibos(&mut owner, &fixture);
        temporal_id = id_archivo(&temporal);
        fs::remove_file(&fixture.contexto).unwrap();

        let resultado = recibo_journal.publicar_con_contexto(&recibo_contexto);
        let ResultadoOwnerPostcheck::PostcheckVerificado { cuerpo } = resultado else {
            panic!("la ausencia de Contexto no debe saltar el postcheck");
        };
        assert!(matches!(&cuerpo.inicial, EstadoContextoJournalV2::Ausente));
        assert!(cuerpo.publicacion.is_none());
        assert!(cuerpo.final_contexto.is_none());
        assert!(!fixture.journal_canonico.exists());
        assert_eq!(id_archivo(&temporal), temporal_id);
    }

    revisar_canarios_y_arbol(&fixture, lock_id);
}

#[test]
fn reread_contexto_falla_y_postcheck_exterior_domina_error_de_publicacion() {
    let temp = tempdir().unwrap();
    let fixture = preparar_fixture_contexto_journal_v2(&temp.path().join("parent"));
    let lock_id;

    {
        let mut lock = LockEscritura::adquirir_en(&fixture.fixture.ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (recibo_contexto, recibo_journal) = preparar_recibos(&mut owner, &fixture);
        let resultado = recibo_journal.publicar_con_contexto_con_prueba(
            &recibo_contexto,
            |_, _, _| {
                fs::write(&fixture.contexto, b"invalid context frame").unwrap();
                Err(ErrorJournalAnclado::ParentChanged)
            },
            |_, cuerpo| {
                assert!(matches!(
                    &cuerpo.inicial,
                    EstadoContextoJournalV2::Observado(_)
                ));
                assert!(matches!(
                    &cuerpo.publicacion,
                    Some(Err(ErrorJournalAnclado::ParentChanged))
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
            panic!("el error del postcheck exterior debe dominar");
        };
        assert!(matches!(error, ErrorJournalAnclado::Io));
        assert!(matches!(
            &cuerpo.inicial,
            EstadoContextoJournalV2::Observado(_)
        ));
        assert!(matches!(
            &cuerpo.publicacion,
            Some(Err(ErrorJournalAnclado::ParentChanged))
        ));
    }

    revisar_canarios_y_arbol(&fixture, lock_id);
}

#[test]
fn postcheck_exterior_domina_no_aplicado_real_y_conserva_temporal() {
    let temp = tempdir().unwrap();
    let fixture = preparar_fixture_contexto_journal_v2(&temp.path().join("parent"));
    let temporal = journal_temporal(&fixture);
    let bytes_journal = fixture.journal.encode().unwrap();
    let lock_id;
    let temporal_id;

    {
        let mut lock = LockEscritura::adquirir_en(&fixture.fixture.ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (recibo_contexto, recibo_journal) = preparar_recibos(&mut owner, &fixture);
        temporal_id = id_archivo(&temporal);
        let resultado = recibo_journal.publicar_con_contexto_con_prueba(
            &recibo_contexto,
            |owner, identidad, journal| {
                let lease = owner.journal_temporal_estricto()?;
                Ok(lease.publicar_v2_con_prueba(identidad, journal, |_, _| {
                    Err(ErrorJournalAnclado::Io)
                }))
            },
            |_, _| Err(ErrorJournalAnclado::Io),
        );

        let ResultadoOwnerPostcheck::PostcheckNoVerificado { cuerpo, error } = resultado else {
            panic!("el postcheck debe dominar NoAplicado");
        };
        assert!(matches!(error, ErrorJournalAnclado::Io));
        assert!(matches!(
            &cuerpo.publicacion,
            Some(Ok(ResultadoPublicacionJournal::NoAplicado(
                ErrorJournalAnclado::Io
            )))
        ));
        assert!(matches!(
            &cuerpo.final_contexto,
            Some(EstadoContextoJournalV2::Observado(_))
        ));
        assert!(!fixture.journal_canonico.exists());
        assert_eq!(fs::read(&temporal).unwrap(), bytes_journal);
        assert_eq!(id_archivo(&temporal), temporal_id);
    }

    revisar_canarios_y_arbol(&fixture, lock_id);
}

#[test]
fn postcheck_exterior_domina_indeterminado_real_y_preserva_ambos_siblings() {
    let temp = tempdir().unwrap();
    let fixture = preparar_fixture_contexto_journal_v2(&temp.path().join("parent"));
    let temporal = journal_temporal(&fixture);
    let bytes_journal = fixture.journal.encode().unwrap();
    let bytes_canonico = b"invalid canonical journal";
    let lock_id;
    let temporal_id;
    let canonical_id = std::cell::Cell::new(None);

    {
        let mut lock = LockEscritura::adquirir_en(&fixture.fixture.ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (recibo_contexto, recibo_journal) = preparar_recibos(&mut owner, &fixture);
        temporal_id = id_archivo(&temporal);
        let resultado = recibo_journal.publicar_con_contexto_con_prueba(
            &recibo_contexto,
            |owner, identidad, journal| {
                let lease = owner.journal_temporal_estricto()?;
                Ok(lease.publicar_v2_con_prueba(identidad, journal, |_, _| {
                    fs::write(&fixture.journal_canonico, bytes_canonico)
                        .map_err(|_| ErrorJournalAnclado::Io)?;
                    canonical_id.set(Some(id_archivo(&fixture.journal_canonico)));
                    Err(ErrorJournalAnclado::Io)
                }))
            },
            |_, _| Err(ErrorJournalAnclado::Io),
        );

        let ResultadoOwnerPostcheck::PostcheckNoVerificado { cuerpo, error } = resultado else {
            panic!("el postcheck debe dominar Indeterminado");
        };
        assert!(matches!(error, ErrorJournalAnclado::Io));
        assert!(matches!(
            &cuerpo.publicacion,
            Some(Ok(ResultadoPublicacionJournal::Indeterminado { .. }))
        ));
        assert!(matches!(
            &cuerpo.final_contexto,
            Some(EstadoContextoJournalV2::Observado(_))
        ));
        assert_eq!(fs::read(&fixture.journal_canonico).unwrap(), bytes_canonico);
        assert_eq!(
            id_archivo(&fixture.journal_canonico),
            canonical_id.get().unwrap()
        );
        assert_eq!(fs::read(&temporal).unwrap(), bytes_journal);
        assert_eq!(id_archivo(&temporal), temporal_id);
    }

    revisar_canarios_y_arbol(&fixture, lock_id);
}
