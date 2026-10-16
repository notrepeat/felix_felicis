use crate::transaccion::anclado::contexto_admision::nombre_temporal_sibling;
use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    contexto_path, id_archivo, materializar, verificar_arbol,
};
use crate::transaccion::anclado::duenio_transaccion::observar_siblings_nuevo_flujo::nombre_intent_retiro;
use crate::transaccion::anclado::identidad_archivo::identidad_archivo;
use crate::transaccion::anclado::{abrir_directorio_absoluto, scope_filesystem};
use crate::transaccion::journal::Journal;
use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::recovery::RecoveryDecision;
use crate::transaccion::lock::LockEscritura;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::PathBuf;
use tempfile::tempdir;

use super::{
    FixtureFilaConservarNew, avanzar_a, capturar_storage, identidad_directorio, preparar_fixture,
    verificar_storage,
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

fn intent_paths(fixture: &FixtureFilaConservarNew) -> (PathBuf, PathBuf) {
    let root = fixture.publicada.ubicacion.ruta();
    let parent = root.parent().unwrap();
    let canonical = nombre_intent_retiro(root.file_name().unwrap()).unwrap();
    let temporary = nombre_temporal_sibling(&canonical).unwrap();
    (parent.join(canonical), parent.join(temporary))
}

#[test]
fn escribe_intent_temporal_solo_tras_prueba_fresca_y_sync() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let (canonical_path, temporary_path) = intent_paths(&fixture);
    let parent_handle =
        abrir_directorio_absoluto(fixture.publicada.ubicacion.ruta().parent().unwrap(), true)
            .unwrap();
    let expected_scope = scope_filesystem(&parent_handle).unwrap();

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let inicio = owner
        .probar_replay_admitido_root_coincidente()
        .unwrap()
        .preparar_retiro_contexto()
        .unwrap();

    let escrito = inicio.escribir_intent_temporal().unwrap();

    assert_eq!(format!("{escrito:?}"), "IntentTemporalEscrito { .. }");
    assert!(!canonical_path.exists());
    let visible = File::open(&temporary_path).unwrap();
    let identidad_self = identidad_archivo(&visible).unwrap();
    let ruta_journal = fixture
        .publicada
        .ubicacion
        .journal()
        .as_path()
        .to_path_buf();
    let ruta_contexto = contexto_path(&fixture.publicada.ubicacion);
    let identidad_journal = id_archivo(&ruta_journal);
    let identidad_contexto = id_archivo(&ruta_contexto);
    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let bytes_contexto = fs::read(&ruta_contexto).unwrap();
    let digest_contexto: [u8; 32] = Sha256::digest(&bytes_contexto).into();
    let frame_esperado = crate::transaccion::anclado::intent_persistido::codificar_nuevo(
        &journal,
        identidad_self,
        identidad_journal,
        identidad_contexto,
        expected_scope.0,
        u64::try_from(bytes_contexto.len()).unwrap(),
        digest_contexto,
    )
    .unwrap();

    assert_eq!(fs::read(&temporary_path).unwrap(), frame_esperado);
    assert_eq!(escrito.datos.identidad_archivo, identidad_self);
    assert!(scope_filesystem(&visible).unwrap() == expected_scope);
    assert_eq!(escrito.frame_esperado, frame_esperado);
    assert_eq!(escrito.datos.longitud, frame_esperado.len());
    let digest_esperado: [u8; 32] = Sha256::digest(&frame_esperado).into();
    assert_eq!(escrito.datos.digest, digest_esperado);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn revalida_el_contexto_antes_de_create_new_y_preserva_mutacion() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let inicio = owner
        .probar_replay_admitido_root_coincidente()
        .unwrap()
        .preparar_retiro_contexto()
        .unwrap();

    let ruta_contexto = contexto_path(&fixture.publicada.ubicacion);
    let id_contexto = id_archivo(&ruta_contexto);
    let mut bytes_contexto = fs::read(&ruta_contexto).unwrap();
    bytes_contexto[0] ^= 1;
    fs::write(&ruta_contexto, &bytes_contexto).unwrap();
    assert!(id_archivo(&ruta_contexto) == id_contexto);
    fixture.publicada.bytes_contexto = bytes_contexto.clone();
    let ids_storage = capturar_storage(&fixture);
    let (canonical_path, temporary_path) = intent_paths(&fixture);

    let error = inicio.escribir_intent_temporal().unwrap_err();

    assert_eq!(error.to_string(), "admission_replay_root_not_proven");
    assert_eq!(format!("{error:?}"), "ErrorPruebaReplayRootCoincidente");
    assert!(!canonical_path.exists());
    assert!(!temporary_path.exists());
    verificar_storage(&fixture, ids_storage);
    assert_eq!(fs::read(&ruta_contexto).unwrap(), bytes_contexto);
}

#[test]
fn revalida_el_journal_antes_de_create_new_y_preserva_mutacion() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let inicio = owner
        .probar_replay_admitido_root_coincidente()
        .unwrap()
        .preparar_retiro_contexto()
        .unwrap();

    let journal_original = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let mut journal_mutado = Journal::new(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        journal_original.parent().clone(),
        journal_original.old().clone(),
        journal_original.new_fingerprint().clone(),
    )
    .unwrap();
    for fase in [
        JournalState::BackupReady,
        JournalState::Installed,
        JournalState::Committed,
    ] {
        journal_mutado.advance(fase).unwrap();
    }
    let bytes_journal_mutado = journal_mutado.encode().unwrap();
    let ruta_journal = fixture
        .publicada
        .ubicacion
        .journal()
        .as_path()
        .to_path_buf();
    let id_journal = id_archivo(&ruta_journal);
    fs::write(&ruta_journal, &bytes_journal_mutado).unwrap();
    assert!(id_archivo(&ruta_journal) == id_journal);
    fixture.publicada.bytes_journal = bytes_journal_mutado.clone();
    let ids_storage = capturar_storage(&fixture);
    let (canonical_path, temporary_path) = intent_paths(&fixture);

    let error = inicio.escribir_intent_temporal().unwrap_err();

    assert_eq!(error.to_string(), "admission_replay_root_not_proven");
    assert_eq!(format!("{error:?}"), "ErrorPruebaReplayRootCoincidente");
    assert!(!canonical_path.exists());
    assert!(!temporary_path.exists());
    verificar_storage(&fixture, ids_storage);
    assert_eq!(fs::read(&ruta_journal).unwrap(), bytes_journal_mutado);
}

fn escribir_intent<'owner, 'lock>(
    owner: &'owner mut DuenioTransaccionAnclado<'lock>,
) -> super::super::super::IntentTemporalEscrito<'owner, 'lock> {
    owner
        .probar_replay_admitido_root_coincidente()
        .unwrap()
        .preparar_retiro_contexto()
        .unwrap()
        .escribir_intent_temporal()
        .unwrap()
}

fn verificar_rechaza(escrito: super::super::super::IntentTemporalEscrito<'_, '_>) {
    let error = escrito.verificar_temporal().unwrap_err();
    assert_eq!(error.to_string(), "admission_replay_root_not_proven");
    assert_eq!(format!("{error:?}"), "ErrorPruebaReplayRootCoincidente");
}

#[test]
fn verifica_el_temporal_visible_por_id_scope_frame_y_eof() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let (_, temporary_path) = intent_paths(&fixture);
    let bytes_esperados = {
        let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let escrito = escribir_intent(&mut owner);
        let bytes = fs::read(&temporary_path).unwrap();

        let verificado = escrito.verificar_temporal().unwrap();
        assert_eq!(format!("{verificado:?}"), "IntentTemporalVerificado { .. }");
        drop(verificado);
        bytes
    };

    assert_eq!(fs::read(&temporary_path).unwrap(), bytes_esperados);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn ausencia_del_nombre_temporal_falla_sin_crearlo() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let (canonical_path, temporary_path) = intent_paths(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let escrito = escribir_intent(&mut owner);
    fs::remove_file(&temporary_path).unwrap();

    verificar_rechaza(escrito);

    assert!(!temporary_path.exists());
    assert!(!canonical_path.exists());
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn mismo_frame_con_id_distinto_falla_y_preserva_original_y_reemplazo() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let (_, temporary_path) = intent_paths(&fixture);
    let retained_path = temporary_path.with_file_name("retained-original-intent");
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let escrito = escribir_intent(&mut owner);
    let bytes_originales = fs::read(&temporary_path).unwrap();
    let identidad_original = id_archivo(&temporary_path);
    fs::rename(&temporary_path, &retained_path).unwrap();
    fs::write(&temporary_path, &bytes_originales).unwrap();
    let identidad_reemplazo = id_archivo(&temporary_path);
    assert_ne!(identidad_original, identidad_reemplazo);

    verificar_rechaza(escrito);

    assert_eq!(fs::read(&retained_path).unwrap(), bytes_originales);
    assert_eq!(fs::read(&temporary_path).unwrap(), bytes_originales);
    assert!(id_archivo(&retained_path) == identidad_original);
    assert!(id_archivo(&temporary_path) == identidad_reemplazo);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn mutacion_in_place_de_igual_longitud_falla_y_preserva_el_archivo() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let (_, temporary_path) = intent_paths(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let escrito = escribir_intent(&mut owner);
    let identidad_original = id_archivo(&temporary_path);
    let bytes_originales = fs::read(&temporary_path).unwrap();
    let mut bytes_mutados = bytes_originales.clone();
    bytes_mutados[0] ^= 1;
    assert_eq!(bytes_mutados.len(), bytes_originales.len());
    fs::write(&temporary_path, &bytes_mutados).unwrap();
    assert!(id_archivo(&temporary_path) == identidad_original);

    verificar_rechaza(escrito);

    assert_eq!(fs::read(&temporary_path).unwrap(), bytes_mutados);
    assert!(id_archivo(&temporary_path) == identidad_original);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn longitudes_cortas_y_extra_fallan_sin_alterar_el_archivo() {
    for agrega_byte in [false, true] {
        let temp = tempdir().unwrap();
        let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
        avanzar_a(&mut fixture, JournalState::Committed);
        let ids_storage = capturar_storage(&fixture);
        let (_, temporary_path) = intent_paths(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let escrito = escribir_intent(&mut owner);
        let identidad_original = id_archivo(&temporary_path);
        let bytes_originales = fs::read(&temporary_path).unwrap();
        let mut bytes_modificados = bytes_originales.clone();
        if agrega_byte {
            bytes_modificados.push(0xa5);
        } else {
            bytes_modificados.pop().unwrap();
        }
        fs::write(&temporary_path, &bytes_modificados).unwrap();
        assert!(id_archivo(&temporary_path) == identidad_original);

        verificar_rechaza(escrito);

        assert_eq!(fs::read(&temporary_path).unwrap(), bytes_modificados);
        assert!(id_archivo(&temporary_path) == identidad_original);
        verificar_storage(&fixture, ids_storage);
    }
}

#[test]
fn parent_movido_con_decoys_reabre_desde_el_handle_retenido() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let parent_original = fixture
        .publicada
        .ubicacion
        .ruta()
        .parent()
        .unwrap()
        .to_path_buf();
    let parent_retenido = parent_original.with_file_name("parent-retained");
    let path_root = fixture.publicada.ubicacion.ruta().to_path_buf();
    let nombre_root = path_root.file_name().unwrap().to_os_string();
    let path_journal = fixture
        .publicada
        .ubicacion
        .journal()
        .as_path()
        .to_path_buf();
    let nombre_journal = path_journal.file_name().unwrap().to_os_string();
    let path_contexto = contexto_path(&fixture.publicada.ubicacion);
    let nombre_contexto = path_contexto.file_name().unwrap().to_os_string();
    let path_lock = fixture.publicada.ubicacion.lock().as_path().to_path_buf();
    let bytes_lock = fs::read(&path_lock).unwrap();
    let bytes_journal = fs::read(&path_journal).unwrap();
    let bytes_contexto = fs::read(&path_contexto).unwrap();
    let identidad_root = identidad_directorio(&fixture.publicada.root);
    let identidad_journal = id_archivo(&path_journal);
    let identidad_contexto = id_archivo(&path_contexto);
    let (_, temporary_path) = intent_paths(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let identidad_guard = lock.identidad_archivo_lock().unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let escrito = escribir_intent(&mut owner);
    let bytes_intent = fs::read(&temporary_path).unwrap();
    let identidad_intent = id_archivo(&temporary_path);

    if let Err(error) = fs::rename(&parent_original, &parent_retenido) {
        #[cfg(windows)]
        {
            assert!(matches!(
                error.raw_os_error(),
                Some(5) | Some(32) | Some(33)
            ));
            assert!(parent_original.is_dir());
            assert!(!parent_retenido.exists());
            drop(escrito);
            assert_eq!(
                owner._lock.identidad_archivo_lock().unwrap(),
                identidad_guard
            );
            drop(owner);
            drop(lock);
            assert_eq!(fs::read(&temporary_path).unwrap(), bytes_intent);
            assert!(id_archivo(&temporary_path) == identidad_intent);
            eprintln!(
                "Windows bloqueó el rename del parent; no se ejercitaron los decoys de intent."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent retenido: {error}");
    }

    fs::create_dir(&parent_original).unwrap();
    let decoy_lock = parent_original.join(path_lock.file_name().unwrap());
    fs::write(&decoy_lock, &bytes_lock).unwrap();
    let identidad_decoy_lock = id_archivo(&decoy_lock);
    assert_ne!(identidad_decoy_lock, identidad_guard);

    let decoy_root = parent_original.join(&nombre_root);
    fs::create_dir(&decoy_root).unwrap();
    fs::write(decoy_root.join("decoy-canary"), b"root decoy").unwrap();
    let decoy_journal = parent_original.join(&nombre_journal);
    fs::write(&decoy_journal, b"journal decoy canary").unwrap();
    let decoy_contexto = parent_original.join(&nombre_contexto);
    fs::write(&decoy_contexto, b"context decoy canary").unwrap();
    fs::write(&temporary_path, b"intent decoy canary").unwrap();

    let verificado = escrito.verificar_temporal().unwrap();
    assert_eq!(format!("{verificado:?}"), "IntentTemporalVerificado { .. }");
    drop(verificado);

    let original_root = parent_retenido.join(&nombre_root);
    let original_journal = parent_retenido.join(&nombre_journal);
    let original_contexto = parent_retenido.join(&nombre_contexto);
    let original_intent = parent_retenido.join(temporary_path.file_name().unwrap());
    assert!(identidad_directorio(&original_root) == identidad_root);
    assert!(id_archivo(&original_journal) == identidad_journal);
    assert!(id_archivo(&original_contexto) == identidad_contexto);
    assert!(id_archivo(&original_intent) == identidad_intent);
    assert_eq!(fs::read(&original_journal).unwrap(), bytes_journal);
    assert_eq!(fs::read(&original_contexto).unwrap(), bytes_contexto);
    assert_eq!(fs::read(&original_intent).unwrap(), bytes_intent);
    assert_eq!(
        fs::read(decoy_root.join("decoy-canary")).unwrap(),
        b"root decoy"
    );
    assert_eq!(fs::read(decoy_journal).unwrap(), b"journal decoy canary");
    assert_eq!(fs::read(decoy_contexto).unwrap(), b"context decoy canary");
    assert_eq!(fs::read(&temporary_path).unwrap(), b"intent decoy canary");
    assert_eq!(fs::read(&decoy_lock).unwrap(), bytes_lock);
}

#[path = "prueba_inicio_retiro_unidad_b_tests.rs"]
mod unidad_b_tests;
