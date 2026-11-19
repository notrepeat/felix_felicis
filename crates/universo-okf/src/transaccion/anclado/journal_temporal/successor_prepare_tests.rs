use super::successor_observation_support as support;
use super::{crear_root, identidad, nombre_journal, nombre_temporal};
use crate::transaccion::anclado::duenio_transaccion::{
    DuenioTransaccionAnclado, ResultadoOwnerPostcheck,
};
use crate::transaccion::anclado::journal_temporal::{
    ErrorJournalAnclado, ReciboTemporalSucesorJournalV2,
};
use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::v2::JournalV2Error;
use crate::transaccion::lock::LockEscritura;
use std::fs;
use tempfile::tempdir;

fn preparar<'original>(
    lock: &mut LockEscritura,
    original: &'original super::super::versionado::ObservacionJournalPersistido,
) -> ResultadoOwnerPostcheck<Result<ReciboTemporalSucesorJournalV2<'original>, ErrorJournalAnclado>>
{
    let mut owner = DuenioTransaccionAnclado::desde_lock(lock).unwrap();
    owner.preparar_sucesor_journal_v2(original)
}

fn error_cuerpo_verificado<'original>(
    resultado: ResultadoOwnerPostcheck<
        Result<ReciboTemporalSucesorJournalV2<'original>, ErrorJournalAnclado>,
    >,
) -> ErrorJournalAnclado {
    match resultado {
        ResultadoOwnerPostcheck::PostcheckVerificado { cuerpo: Err(error) } => error,
        _ => panic!("se esperaba un error de preparación con postcheck verificado"),
    }
}

#[test]
fn prepara_los_tres_sucesores_contiguos_y_preserva_observacion_y_canario() {
    for (indice, estado) in [
        JournalState::Prepared,
        JournalState::BackupReady,
        JournalState::Installed,
    ]
    .into_iter()
    .enumerate()
    {
        let temp = tempdir().unwrap();
        let root = crear_root(temp.path());
        let canario = support::crear_canario(&root);
        let mut lock = LockEscritura::adquirir(&root).unwrap();
        let transaction_id = format!("{indice:032x}");
        let (original, bytes_original, id_original, observacion) =
            support::observar_v2_en_fase(&root, &mut lock, &transaction_id, estado);
        let canonic = nombre_journal(&root);
        let temporal = nombre_temporal(temp.path());
        let mut candidato = original.clone();
        let siguiente = match estado {
            JournalState::Prepared => JournalState::BackupReady,
            JournalState::BackupReady => JournalState::Installed,
            JournalState::Installed => JournalState::Committed,
            JournalState::Committed => unreachable!(),
        };
        candidato.advance(siguiente).unwrap();
        original.validate_successor(&candidato).unwrap();
        let bytes_candidato = candidato.encode().unwrap();

        let resultado = preparar(&mut lock, &observacion);
        let identidad_temporal = match resultado {
            ResultadoOwnerPostcheck::PostcheckVerificado { cuerpo: Ok(recibo) } => {
                assert!(std::ptr::eq(recibo.original, &observacion));
                assert!(recibo.candidato == candidato);
                assert_eq!(recibo.original.identidad(), id_original);
                assert_eq!(recibo.identidad_temporal, identidad(&temporal));
                assert_eq!(recibo.temporal.identidad(), recibo.identidad_temporal);
                assert!(recibo.temporal.journal() == &candidato);
                assert_eq!(fs::read(&canonic).unwrap(), bytes_original);
                assert_eq!(fs::read(&temporal).unwrap(), bytes_candidato);
                let identidad = recibo.identidad_temporal;
                drop(recibo);
                identidad
            }
            _ => panic!("se esperaba el recibo del sucesor tras el postcheck"),
        };

        drop(observacion);
        drop(lock);
        support::comprobar_canario(&canario);
        assert_eq!(identidad(&canonic), id_original);
        assert_eq!(fs::read(&canonic).unwrap(), bytes_original);
        assert_eq!(identidad(&temporal), identidad_temporal);
        assert_eq!(fs::read(temporal).unwrap(), bytes_candidato);
    }
}

#[test]
fn rechaza_v1_y_committed_sin_temporal_y_retorna_el_error_bajo_postcheck() {
    for committed in [false, true] {
        let temp = tempdir().unwrap();
        let root = crear_root(temp.path());
        let canario = support::crear_canario(&root);
        let mut lock = LockEscritura::adquirir(&root).unwrap();
        let (bytes_original, id_original, observacion, error_esperado) = if committed {
            let (_, bytes, identidad, observacion) = support::observar_v2_en_fase(
                &root,
                &mut lock,
                "0123456789abcdef0123456789abcdef",
                JournalState::Committed,
            );
            (
                bytes,
                identidad,
                observacion,
                ErrorJournalAnclado::InvalidJournalV2(JournalV2Error::TransitionInvalid),
            )
        } else {
            let (_, bytes, identidad, observacion) = support::observar_v1(&root, &mut lock);
            (
                bytes,
                identidad,
                observacion,
                ErrorJournalAnclado::JournalMismatch,
            )
        };
        let canonic = nombre_journal(&root);
        let temporal = nombre_temporal(temp.path());

        assert_eq!(
            error_cuerpo_verificado(preparar(&mut lock, &observacion)),
            error_esperado
        );
        assert_eq!(observacion.identidad(), id_original);
        assert_eq!(fs::read(&canonic).unwrap(), bytes_original);
        drop(observacion);
        drop(lock);

        support::comprobar_canario(&canario);
        assert!(!temporal.exists());
        assert_eq!(identidad(&canonic), id_original);
        assert_eq!(fs::read(canonic).unwrap(), bytes_original);
    }
}

#[test]
fn canónico_reemplazado_por_bytes_iguales_y_id_nuevo_no_crea_temporal() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let canario = support::crear_canario(&root);
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let (_, bytes_original, id_original, observacion) = support::observar_v2_en_fase(
        &root,
        &mut lock,
        "0123456789abcdef0123456789abcdef",
        JournalState::Prepared,
    );
    let canonic = nombre_journal(&root);
    let anterior = root.join("journal-anterior");
    let temporal = nombre_temporal(temp.path());
    fs::rename(&canonic, &anterior).unwrap();
    fs::write(&canonic, &bytes_original).unwrap();
    let id_reemplazo = identidad(&canonic);
    assert_ne!(id_reemplazo, id_original);

    assert_eq!(
        error_cuerpo_verificado(preparar(&mut lock, &observacion)),
        ErrorJournalAnclado::IdentityMismatch
    );
    drop(observacion);
    drop(lock);

    support::comprobar_canario(&canario);
    assert!(!temporal.exists());
    assert_eq!(identidad(&anterior), id_original);
    assert_eq!(fs::read(&anterior).unwrap(), bytes_original);
    assert_eq!(identidad(&canonic), id_reemplazo);
    assert_eq!(fs::read(canonic).unwrap(), bytes_original);
}

#[test]
fn canónico_con_mismo_id_y_registro_distinto_no_crea_temporal() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let canario = support::crear_canario(&root);
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let (_, bytes_original, id_original, observacion) = support::observar_v2_en_fase(
        &root,
        &mut lock,
        "0123456789abcdef0123456789abcdef",
        JournalState::Prepared,
    );
    let canonic = nombre_journal(&root);
    let temporal = nombre_temporal(temp.path());
    let registro_distinto = {
        let lease = lock.journal_temporal_anclado().unwrap();
        super::journal_v2_prueba(&lease, "fedcba9876543210fedcba9876543210")
    };
    let bytes_distintos = registro_distinto.encode().unwrap();
    fs::write(&canonic, &bytes_distintos).unwrap();
    assert_eq!(identidad(&canonic), id_original);
    assert_ne!(bytes_distintos, bytes_original);

    assert_eq!(
        error_cuerpo_verificado(preparar(&mut lock, &observacion)),
        ErrorJournalAnclado::JournalMismatch
    );
    drop(observacion);
    drop(lock);

    support::comprobar_canario(&canario);
    assert!(!temporal.exists());
    assert_eq!(identidad(&canonic), id_original);
    assert_eq!(fs::read(canonic).unwrap(), bytes_distintos);
}

#[test]
fn temporal_ocupado_preserva_su_id_bytes_y_el_canonico() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let canario = support::crear_canario(&root);
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let (_, bytes_original, id_original, observacion) = support::observar_v2_en_fase(
        &root,
        &mut lock,
        "0123456789abcdef0123456789abcdef",
        JournalState::Prepared,
    );
    let canonic = nombre_journal(&root);
    let temporal = nombre_temporal(temp.path());
    let bytes_ocupante = b"temporal ocupante preservado";
    fs::write(&temporal, bytes_ocupante).unwrap();
    let id_ocupante = identidad(&temporal);

    assert_eq!(
        error_cuerpo_verificado(preparar(&mut lock, &observacion)),
        ErrorJournalAnclado::Exists
    );
    drop(observacion);
    drop(lock);

    support::comprobar_canario(&canario);
    assert_eq!(identidad(&temporal), id_ocupante);
    assert_eq!(fs::read(&temporal).unwrap(), bytes_ocupante);
    assert_eq!(identidad(&canonic), id_original);
    assert_eq!(fs::read(canonic).unwrap(), bytes_original);
}
