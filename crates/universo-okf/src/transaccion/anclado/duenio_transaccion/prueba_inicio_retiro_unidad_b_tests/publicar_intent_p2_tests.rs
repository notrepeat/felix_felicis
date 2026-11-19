use super::*;
use crate::transaccion::anclado::IdentidadArchivo;
use crate::transaccion::anclado::contexto_admision::nombre_temporal_sibling;
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::verificar_arbol;
use crate::transaccion::journal::{Journal, JournalState};
use crate::transaccion::snapshot::TipoEntradaSnapshot;
use std::cell::Cell;
use std::fs;
use std::path::PathBuf;

#[test]
fn canonic_preexistente_despues_de_observar_bloquea_antes_del_syscall() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let (canonico, temporal) = intent_paths(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let escrito = escribir_intent(&mut owner);
    let bytes_intent = fs::read(&temporal).unwrap();
    let id_intent = id_archivo(&temporal);
    let observacion = escrito
        .verificar_temporal()
        .unwrap()
        .revalidar_completo()
        .unwrap();
    let canary = b"canonical created after full observation";
    fs::write(&canonico, canary).unwrap();
    let id_canary = id_archivo(&canonico);
    let llamadas = Cell::new(0);

    assert_rechazo(observacion.publicar_intent_con(|parent, source, from, to| {
        llamadas.set(llamadas.get() + 1);
        renombrar_real(parent, source, from, to)
    }));

    assert_eq!(llamadas.get(), 0);
    assert_eq!(fs::read(&canonico).unwrap(), canary);
    assert!(id_archivo(&canonico) == id_canary);
    assert_eq!(fs::read(&temporal).unwrap(), bytes_intent);
    assert!(id_archivo(&temporal) == id_intent);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn syscall_sin_efecto_y_con_error_preserva_temporal_y_almacenamiento() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let (canonico, temporal) = intent_paths(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let escrito = escribir_intent(&mut owner);
    let bytes_intent = fs::read(&temporal).unwrap();
    let id_intent = id_archivo(&temporal);
    let observacion = escrito
        .verificar_temporal()
        .unwrap()
        .revalidar_completo()
        .unwrap();
    let llamadas = Cell::new(0);

    assert_rechazo(observacion.publicar_intent_con(|_, _, _, _| {
        llamadas.set(llamadas.get() + 1);
        Err(())
    }));

    assert_eq!(llamadas.get(), 1);
    assert!(!canonico.exists());
    assert_eq!(fs::read(&temporal).unwrap(), bytes_intent);
    assert!(id_archivo(&temporal) == id_intent);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn revalidacion_entre_observacion_y_publicador_falla_antes_del_syscall() {
    #[derive(Clone, Copy)]
    enum Cambio {
        Journal,
        Contexto,
        Root,
    }

    for cambio in [Cambio::Journal, Cambio::Contexto, Cambio::Root] {
        let temp = tempdir().unwrap();
        let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
        avanzar_a(&mut fixture, JournalState::Committed);
        let (canonico, temporal) = intent_paths(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let escrito = escribir_intent(&mut owner);
        let bytes_intent = fs::read(&temporal).unwrap();
        let id_intent = id_archivo(&temporal);
        let observacion = escrito
            .verificar_temporal()
            .unwrap()
            .revalidar_completo()
            .unwrap();
        let path_contexto = contexto_path(&fixture.publicada.ubicacion);
        let path_journal = fixture
            .publicada
            .ubicacion
            .journal()
            .as_path()
            .to_path_buf();
        let id_contexto_original = id_archivo(&path_contexto);
        let id_journal_original = id_archivo(&path_journal);
        let id_root_original = identidad_directorio(&fixture.publicada.root);
        let mut bytes_root_mutados = None;

        match cambio {
            Cambio::Journal => {
                let original = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
                let mut journal = Journal::new(
                    "dddddddddddddddddddddddddddddddd",
                    original.parent().clone(),
                    original.old().clone(),
                    original.new_fingerprint().clone(),
                )
                .unwrap();
                for fase in [
                    JournalState::BackupReady,
                    JournalState::Installed,
                    JournalState::Committed,
                ] {
                    journal.advance(fase).unwrap();
                }
                let bytes = journal.encode().unwrap();
                fs::write(&path_journal, &bytes).unwrap();
                fixture.publicada.bytes_journal = bytes;
            }
            Cambio::Contexto => {
                let mut bytes = fs::read(&path_contexto).unwrap();
                bytes[0] ^= 1;
                fs::write(&path_contexto, &bytes).unwrap();
                fixture.publicada.bytes_contexto = bytes;
            }
            Cambio::Root => {
                let entrada = fixture
                    .publicada
                    .entradas
                    .iter()
                    .find(|entrada| entrada.tipo() == TipoEntradaSnapshot::Archivo)
                    .unwrap();
                let path = fixture.publicada.root.join(entrada.ruta());
                let mut bytes = fs::read(&path).unwrap();
                bytes.push(0xa5);
                fs::write(path, &bytes).unwrap();
                bytes_root_mutados = Some((std::path::PathBuf::from(entrada.ruta()), bytes));
            }
        }

        let ids_storage = capturar_storage(&fixture);
        let llamadas = Cell::new(0);
        assert_rechazo(observacion.publicar_intent_con(|parent, source, from, to| {
            llamadas.set(llamadas.get() + 1);
            renombrar_real(parent, source, from, to)
        }));

        assert_eq!(llamadas.get(), 0);
        assert!(!canonico.exists());
        assert_eq!(fs::read(&temporal).unwrap(), bytes_intent);
        assert!(id_archivo(&temporal) == id_intent);
        assert_eq!(
            fs::read(&path_contexto).unwrap(),
            fixture.publicada.bytes_contexto
        );
        assert_eq!(
            fs::read(&path_journal).unwrap(),
            fixture.publicada.bytes_journal
        );
        assert!(id_archivo(&path_contexto) == ids_storage.0);
        assert!(id_archivo(&path_journal) == ids_storage.1);
        assert!(id_contexto_original == ids_storage.0);
        assert!(id_journal_original == ids_storage.1);
        assert!(identidad_directorio(&fixture.publicada.root) == id_root_original);
        if let Some((ruta, bytes)) = bytes_root_mutados {
            assert_eq!(fs::read(fixture.publicada.root.join(ruta)).unwrap(), bytes);
        } else {
            verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
        }
    }
}

#[test]
fn parent_movido_publica_desde_el_original_y_preserva_decoys() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let path_root = fixture.publicada.root.clone();
    let parent_original = path_root.parent().unwrap().to_path_buf();
    let parent_retenido = parent_original.with_file_name("parent-retained");
    let nombre_root = path_root.file_name().unwrap().to_os_string();
    let path_journal = fixture
        .publicada
        .ubicacion
        .journal()
        .as_path()
        .to_path_buf();
    let nombre_journal = path_journal.file_name().unwrap().to_os_string();
    let nombre_journal_temporal = fixture.publicada.ubicacion.nombre_journal_temporal();
    let path_contexto = contexto_path(&fixture.publicada.ubicacion);
    let nombre_contexto = path_contexto.file_name().unwrap().to_os_string();
    let nombre_contexto_temporal = nombre_temporal_sibling(&nombre_contexto).unwrap();
    let (path_intent_canonico, path_intent_temporal) = intent_paths(&fixture);
    let nombre_intent_canonico = path_intent_canonico.file_name().unwrap().to_os_string();
    let nombre_intent_temporal = path_intent_temporal.file_name().unwrap().to_os_string();
    let path_lock = fixture.publicada.ubicacion.lock().as_path().to_path_buf();
    let bytes_lock = fs::read(&path_lock).unwrap();
    let id_lock = id_archivo(&path_lock);
    let bytes_journal = fs::read(&path_journal).unwrap();
    let bytes_contexto = fs::read(&path_contexto).unwrap();
    let id_journal = id_archivo(&path_journal);
    let id_contexto = id_archivo(&path_contexto);
    let id_root = identidad_directorio(&path_root);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let identidad_guard = lock.identidad_archivo_lock().unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let escrito = escribir_intent(&mut owner);
    let bytes_intent = fs::read(&path_intent_temporal).unwrap();
    let id_intent = id_archivo(&path_intent_temporal);
    let observacion = escrito
        .verificar_temporal()
        .unwrap()
        .revalidar_completo()
        .unwrap();

    if let Err(error) = fs::rename(&parent_original, &parent_retenido) {
        #[cfg(windows)]
        {
            assert!(matches!(
                error.raw_os_error(),
                Some(5) | Some(32) | Some(33)
            ));
            assert!(parent_original.is_dir());
            assert!(!parent_retenido.exists());
            drop(observacion);
            assert!(owner._lock.identidad_archivo_lock().unwrap() == identidad_guard);
            drop(owner);
            drop(lock);
            assert_eq!(fs::read(&path_intent_temporal).unwrap(), bytes_intent);
            assert!(id_archivo(&path_intent_temporal) == id_intent);
            assert_eq!(fs::read(&path_lock).unwrap(), bytes_lock);
            assert!(id_archivo(&path_lock) == id_lock);
            assert_eq!(fs::read(&path_journal).unwrap(), bytes_journal);
            assert_eq!(fs::read(&path_contexto).unwrap(), bytes_contexto);
            assert!(id_archivo(&path_journal) == id_journal);
            assert!(id_archivo(&path_contexto) == id_contexto);
            verificar_arbol(&path_root, &fixture.publicada.entradas);
            eprintln!(
                "Windows bloqueó el rename del parent; no se ejercitaron los decoys del publicador."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent retenido: {error}");
    }

    fs::create_dir(&parent_original).unwrap();
    let decoy_root = parent_original.join(&nombre_root);
    fs::create_dir(&decoy_root).unwrap();
    fs::write(decoy_root.join("decoy-canary"), b"root decoy canary").unwrap();
    let id_decoy_root = identidad_directorio(&decoy_root);
    assert!(id_decoy_root != id_root);

    let decoys = vec![
        write_canary(parent_original.join(nombre_lock(&path_lock)), &bytes_lock),
        write_canary(parent_original.join(&nombre_journal), b"journal decoy"),
        write_canary(
            parent_original.join(&nombre_journal_temporal),
            b"journal temporary decoy",
        ),
        write_canary(parent_original.join(&nombre_contexto), b"context decoy"),
        write_canary(
            parent_original.join(&nombre_contexto_temporal),
            b"context temporary decoy",
        ),
        write_canary(
            parent_original.join(&nombre_intent_canonico),
            b"intent canonical decoy",
        ),
        write_canary(
            parent_original.join(&nombre_intent_temporal),
            b"intent temporary decoy",
        ),
    ];
    let id_lock_decoy = decoys[0].2;
    assert!(id_lock_decoy != identidad_guard);
    let id_intent_canonico_decoy = decoys[5].2;
    let id_intent_temporal_decoy = decoys[6].2;
    let llamadas = Cell::new(0);

    let publicado = observacion
        .publicar_intent_con(|parent, source, from, to| {
            llamadas.set(llamadas.get() + 1);
            renombrar_real(parent, source, from, to)
        })
        .unwrap();

    assert_eq!(llamadas.get(), 1);
    drop(publicado);
    drop(owner);
    drop(lock);

    let root_original = parent_retenido.join(&nombre_root);
    let journal_original = parent_retenido.join(&nombre_journal);
    let context_original = parent_retenido.join(&nombre_contexto);
    let intent_canonico_original = parent_retenido.join(&nombre_intent_canonico);
    let intent_temporal_original = parent_retenido.join(&nombre_intent_temporal);
    let lock_original = parent_retenido.join(nombre_lock(&path_lock));
    assert!(identidad_directorio(&root_original) == id_root);
    assert!(id_archivo(&journal_original) == id_journal);
    assert!(id_archivo(&context_original) == id_contexto);
    assert_eq!(fs::read(&journal_original).unwrap(), bytes_journal);
    assert_eq!(fs::read(&context_original).unwrap(), bytes_contexto);
    assert_eq!(fs::read(&intent_canonico_original).unwrap(), bytes_intent);
    assert!(id_archivo(&intent_canonico_original) == id_intent);
    assert!(!intent_temporal_original.exists());
    assert_eq!(fs::read(&lock_original).unwrap(), bytes_lock);
    assert!(id_archivo(&lock_original) == id_lock);
    verificar_arbol(&root_original, &fixture.publicada.entradas);
    assert_eq!(
        fs::read(decoy_root.join("decoy-canary")).unwrap(),
        b"root decoy canary"
    );
    assert!(identidad_directorio(&decoy_root) == id_decoy_root);
    for (path, bytes, identity) in decoys {
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(id_archivo(&path) == identity);
    }
    assert!(id_intent_canonico_decoy != id_intent);
    assert!(id_intent_temporal_decoy != id_intent);
}

fn nombre_lock(path: &std::path::Path) -> std::ffi::OsString {
    path.file_name().unwrap().to_os_string()
}

fn write_canary(path: PathBuf, bytes: &[u8]) -> (PathBuf, Vec<u8>, IdentidadArchivo) {
    fs::write(&path, bytes).unwrap();
    let identity = id_archivo(&path);
    (path, bytes.to_vec(), identity)
}
