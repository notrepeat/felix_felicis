use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    contexto_path, id_archivo, materializar, verificar_arbol,
};
use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::recovery::RecoveryDecision;
use crate::transaccion::lock::LockEscritura;
use std::fs;
use tempfile::tempdir;

use super::{
    avanzar_a, capturar_storage, identidad_directorio, preparar_fixture, verificar_storage,
};

#[test]
fn prepara_solo_prueba_committed_con_root_new_y_siblings_exactos() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let id_root = identidad_directorio(&fixture.publicada.root);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();

    let inicio = prueba.preparar_retiro_contexto().unwrap();

    assert_eq!(format!("{inicio:?}"), "PruebaInicioRetiroContexto { .. }");
    verificar_storage(&fixture, ids_storage);
    assert!(identidad_directorio(&fixture.publicada.root) == id_root);
}

#[test]
fn rechaza_installed_sin_modificar_storage() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Installed);
    let ids_storage = capturar_storage(&fixture);
    let id_root = identidad_directorio(&fixture.publicada.root);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();

    assert!(prueba.preparar_retiro_contexto().is_err());

    verificar_storage(&fixture, ids_storage);
    assert!(identidad_directorio(&fixture.publicada.root) == id_root);
}

#[test]
fn rechaza_backup_old_aunque_la_fila_conservar_new_lo_acepte() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), true);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let ruta_backup = fixture.publicada.ubicacion.backup().as_path().to_path_buf();
    let id_backup = fixture.identidad_backup_old.as_ref().unwrap().clone();

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();
    let fila = prueba.observar_fila_conservar_new().unwrap();
    assert_eq!(fila.decision, RecoveryDecision::KeepValidatedCommittedNew);
    drop(fila);
    drop(owner);
    drop(lock);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();
    assert!(prueba.preparar_retiro_contexto().is_err());

    verificar_storage(&fixture, ids_storage);
    assert!(identidad_directorio(&ruta_backup) == id_backup);
    verificar_arbol(
        &ruta_backup,
        fixture.entradas_old.as_ref().expect("backup Old fixture"),
    );
}

#[test]
fn rechaza_stage_presente_y_preserva_su_arbol() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let journal =
        crate::transaccion::journal::Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let ruta_stage = fixture
        .publicada
        .ubicacion
        .stage(journal.transaction_id())
        .unwrap()
        .as_path()
        .to_path_buf();
    fs::create_dir(&ruta_stage).unwrap();
    materializar(&ruta_stage, &fixture.publicada.entradas);
    let id_stage = identidad_directorio(&ruta_stage);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();
    let ids_storage = capturar_storage(&fixture);

    assert!(prueba.preparar_retiro_contexto().is_err());

    verificar_storage(&fixture, ids_storage);
    assert!(identidad_directorio(&ruta_stage) == id_stage);
    verificar_arbol(&ruta_stage, &fixture.publicada.entradas);
}

#[test]
fn rechaza_contexto_mutado_despues_de_emitir_prueba_y_preserva_bytes_mutados() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();

    let ruta_contexto = contexto_path(&fixture.publicada.ubicacion);
    let id_contexto_antes = id_archivo(&ruta_contexto);
    let mut bytes_mutados = fs::read(&ruta_contexto).unwrap();
    bytes_mutados[0] ^= 1;
    fs::write(&ruta_contexto, &bytes_mutados).unwrap();
    assert!(id_archivo(&ruta_contexto) == id_contexto_antes);
    fixture.publicada.bytes_contexto = bytes_mutados.clone();
    let ids_storage = capturar_storage(&fixture);

    assert!(prueba.preparar_retiro_contexto().is_err());

    verificar_storage(&fixture, ids_storage);
    assert_eq!(fs::read(&ruta_contexto).unwrap(), bytes_mutados);
}
