use super::successor_observation_support as support;
use super::{crear_root, identidad, nombre_journal, nombre_temporal};
use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::anclado::journal_temporal::identidad_archivo;
use crate::transaccion::journal::JournalState;
use crate::transaccion::lock::LockEscritura;
use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use tempfile::tempdir;

#[test]
fn reemplazo_temporal_bytes_iguales_id_nuevo_falla_y_preserva_ambos() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let canary = support::crear_canario(&root);
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let (original, bytes_original, id_original, observacion) = support::observar_v2_en_fase(
        &root,
        &mut lock,
        "0123456789abcdef0123456789abcdef",
        JournalState::Prepared,
    );
    let mut candidato = original;
    candidato.advance(JournalState::BackupReady).unwrap();
    let bytes_candidato = candidato.encode().unwrap();
    let canonic = nombre_journal(&root);
    let temporal = nombre_temporal(temp.path());
    let temporal_creado = temp.path().join("temporal-creado-original");
    let id_creado = RefCell::new(None);
    let id_reemplazo = RefCell::new(None);
    let archivo_creado = RefCell::new(None);

    let resultado = {
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let lease = owner.journal_temporal_estricto().unwrap();
        lease.preparar_sucesor_temporal_v2_con_prueba(&observacion, || {
            let archivo = File::open(&temporal).unwrap();
            *id_creado.borrow_mut() = Some(identidad_archivo(&archivo).unwrap());
            *archivo_creado.borrow_mut() = Some(archivo);
            fs::rename(&temporal, &temporal_creado).unwrap();
            fs::write(&temporal, &bytes_candidato).unwrap();
            *id_reemplazo.borrow_mut() = Some(identidad(&temporal));
        })
    };

    assert!(matches!(
        resultado,
        Err(super::super::ErrorJournalAnclado::IdentityMismatch)
    ));
    let id_creado = id_creado.into_inner().expect("el temporal se creó");
    let id_reemplazo = id_reemplazo.into_inner().expect("el reemplazo se escribió");
    let mut archivo_creado = archivo_creado
        .into_inner()
        .expect("se retuvo el handle original");
    assert_ne!(id_creado, id_reemplazo);
    assert_eq!(identidad_archivo(&archivo_creado).unwrap(), id_creado);
    drop(observacion);
    drop(lock);

    assert_eq!(identidad_archivo(&archivo_creado).unwrap(), id_creado);
    archivo_creado.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes_handle = Vec::new();
    archivo_creado.read_to_end(&mut bytes_handle).unwrap();
    assert_eq!(bytes_handle, bytes_candidato);
    assert_eq!(identidad(&temporal_creado), id_creado);
    assert_eq!(identidad(&temporal), id_reemplazo);
    assert_eq!(fs::read(&temporal_creado).unwrap(), bytes_candidato);
    assert_eq!(fs::read(&temporal).unwrap(), bytes_candidato);
    drop(archivo_creado);

    support::comprobar_canario(&canary);
    assert_eq!(identidad(&canonic), id_original);
    assert_eq!(fs::read(canonic).unwrap(), bytes_original);
}

#[test]
fn parent_distinto_rechaza_y_preserva_canónicos_y_temporales_ambos() {
    let temp = tempdir().unwrap();
    let parent_a = temp.path().join("parent-a");
    let parent_b = temp.path().join("parent-b");
    fs::create_dir(&parent_a).unwrap();
    fs::create_dir(&parent_b).unwrap();
    let root_a = crear_root(&parent_a);
    let root_b = crear_root(&parent_b);
    let canary_a = support::crear_canario(&root_a);
    let canary_b = support::crear_canario(&root_b);

    let mut lock_a = LockEscritura::adquirir(&root_a).unwrap();
    let (journal_a, bytes_a, id_a, observacion_a) = support::observar_v2_en_fase(
        &root_a,
        &mut lock_a,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        JournalState::Prepared,
    );
    drop(lock_a);

    let mut lock_b = LockEscritura::adquirir(&root_b).unwrap();
    let (journal_b, bytes_b, id_b, observacion_b) = support::observar_v2_en_fase(
        &root_b,
        &mut lock_b,
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        JournalState::Prepared,
    );
    let resultado = {
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock_b).unwrap();
        let lease = owner.journal_temporal_estricto().unwrap();
        lease.preparar_sucesor_temporal_v2(&observacion_a)
    };

    assert!(matches!(
        resultado,
        Err(super::super::ErrorJournalAnclado::ParentIdentityMismatch)
    ));
    assert!(
        journal_a.parent() != journal_b.parent(),
        "las identidades nativas de parent deben diferir"
    );
    drop(observacion_a);
    drop(observacion_b);
    drop(lock_b);
    support::comprobar_canario(&canary_a);
    support::comprobar_canario(&canary_b);

    let canonic_a = nombre_journal(&root_a);
    let canonic_b = nombre_journal(&root_b);
    assert_eq!(identidad(&canonic_a), id_a);
    assert_eq!(fs::read(canonic_a).unwrap(), bytes_a);
    assert_eq!(identidad(&canonic_b), id_b);
    assert_eq!(fs::read(canonic_b).unwrap(), bytes_b);
    assert!(!nombre_temporal(&parent_a).exists());
    assert!(!nombre_temporal(&parent_b).exists());
}

#[test]
fn scope_ausente_rechaza_antes_de_crear_temporal() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let canary = support::crear_canario(&root);
    let mut strict_lock = LockEscritura::adquirir(&root).unwrap();
    let (_, bytes_original, id_original, observacion) = support::observar_v2_en_fase(
        &root,
        &mut strict_lock,
        "0123456789abcdef0123456789abcdef",
        JournalState::Prepared,
    );
    drop(strict_lock);

    let mut legacy_lock = LockEscritura::adquirir(&root).unwrap();
    let resultado = {
        let lease = legacy_lock.journal_temporal_anclado().unwrap();
        lease.preparar_sucesor_temporal_v2(&observacion)
    };
    assert!(matches!(
        resultado,
        Err(super::super::ErrorJournalAnclado::ParentScopeUnverified)
    ));
    drop(observacion);
    drop(legacy_lock);

    support::comprobar_canario(&canary);
    let canonic = nombre_journal(&root);
    assert_eq!(identidad(&canonic), id_original);
    assert_eq!(fs::read(canonic).unwrap(), bytes_original);
    assert!(!nombre_temporal(temp.path()).exists());
}
