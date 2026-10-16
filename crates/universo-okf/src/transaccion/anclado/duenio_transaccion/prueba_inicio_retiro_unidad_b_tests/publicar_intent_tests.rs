use super::*;
use std::cell::Cell;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::Write;

#[test]
fn publica_una_vez_y_confirma_el_intent_visible() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let (canonical, temporary) = intent_paths(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let escrito = escribir_intent(&mut owner);
    let bytes_intent = std::fs::read(&temporary).unwrap();
    let id_intent = id_archivo(&temporary);
    let observacion = escrito
        .verificar_temporal()
        .unwrap()
        .revalidar_completo()
        .unwrap();

    let publicado = observacion.publicar_intent().unwrap();

    assert_eq!(format!("{publicado:?}"), "IntentPublicado { .. }");
    assert_eq!(std::fs::read(&canonical).unwrap(), bytes_intent);
    assert!(id_archivo(&canonical) == id_intent);
    assert!(!temporary.exists());
    drop(publicado);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn canary_entre_gate_y_syscall_preserva_canary_y_temporal() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let (canonical, temporary) = intent_paths(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let escrito = escribir_intent(&mut owner);
    let bytes_intent = std::fs::read(&temporary).unwrap();
    let id_intent = id_archivo(&temporary);
    let observacion = escrito
        .verificar_temporal()
        .unwrap()
        .revalidar_completo()
        .unwrap();
    let canary = b"canonical canary inserted after the full gate";
    let llamadas = Cell::new(0);

    assert_rechazo(observacion.publicar_intent_con(|parent, source, from, to| {
        llamadas.set(llamadas.get() + 1);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&canonical)
            .unwrap();
        file.write_all(canary).unwrap();
        file.sync_all().unwrap();
        drop(file);
        renombrar_real(parent, source, from, to)
    }));

    assert_eq!(llamadas.get(), 1);
    assert_eq!(std::fs::read(&canonical).unwrap(), canary);
    assert!(id_archivo(&canonical) != id_intent);
    assert_eq!(std::fs::read(&temporary).unwrap(), bytes_intent);
    assert!(id_archivo(&temporary) == id_intent);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn renombre_aplicado_con_error_del_syscall_se_confirma_por_postcondiciones() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let (canonical, temporary) = intent_paths(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let escrito = escribir_intent(&mut owner);
    let bytes_intent = std::fs::read(&temporary).unwrap();
    let id_intent = id_archivo(&temporary);
    let observacion = escrito
        .verificar_temporal()
        .unwrap()
        .revalidar_completo()
        .unwrap();
    let llamadas = Cell::new(0);

    let publicado = observacion
        .publicar_intent_con(|parent, source, from, to| {
            llamadas.set(llamadas.get() + 1);
            renombrar_real(parent, source, from, to)?;
            Err(())
        })
        .unwrap();

    assert_eq!(llamadas.get(), 1);
    assert_eq!(format!("{publicado:?}"), "IntentPublicado { .. }");
    assert_eq!(std::fs::read(&canonical).unwrap(), bytes_intent);
    assert!(id_archivo(&canonical) == id_intent);
    assert!(!temporary.exists());
    drop(publicado);
    verificar_storage(&fixture, ids_storage);
}

fn renombrar_real(parent: &File, source: &File, from: &OsStr, to: &OsStr) -> Result<(), ()> {
    crate::transaccion::anclado::contexto_admision::renombrar_no_reemplazar(
        parent, source, from, to,
    )
}

#[path = "publicar_intent_p2_tests.rs"]
mod p2_tests;

#[path = "publicar_intent_lector_tests.rs"]
mod lector_tests;
