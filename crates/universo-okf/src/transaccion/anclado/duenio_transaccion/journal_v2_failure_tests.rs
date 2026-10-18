use super::*;
use crate::transaccion::anclado::journal_temporal::ErrorJournalAnclado;
use std::fs;
use tempfile::tempdir;

#[test]
fn legacy_scope_unverified_rejects_v2_temporary_without_creation() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let temporal = ruta_temporal(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());

    {
        let lease = owner.journal_temporal().unwrap();
        let error = lease.crear_temporal_v2(&journal).err().unwrap();
        assert_eq!(error, ErrorJournalAnclado::ParentScopeUnverified);
    }
    assert!(!temporal.exists());
}

#[test]
fn occupied_v2_temporal_preserves_bytes_and_native_identity() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let temporal = ruta_temporal(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
    let canary = b"occupied-temporal-canary";
    fs::write(&temporal, canary).unwrap();
    let identity_before = identidad_visible(&temporal);

    let error = owner.crear_temporal_journal_v2(&journal).err().unwrap();
    assert_eq!(error, ErrorJournalAnclado::Exists);
    assert_eq!(fs::read(&temporal).unwrap(), canary);
    assert_eq!(identidad_visible(&temporal), identity_before);
}
