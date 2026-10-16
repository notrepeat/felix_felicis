use super::*;

fn assert_rechazo<T, E>(resultado: Result<T, E>)
where
    T: std::fmt::Debug,
    E: std::fmt::Debug + std::fmt::Display,
{
    let error = resultado.unwrap_err();
    assert_eq!(error.to_string(), "admission_replay_root_not_proven");
    assert_eq!(format!("{error:?}"), "ErrorPruebaReplayRootCoincidente");
}

#[test]
fn revalida_intent_committed_completo_y_preserva_el_almacenamiento() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let (canonical_path, temporary_path) = intent_paths(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let escrito = escribir_intent(&mut owner);
    let bytes_intent = fs::read(&temporary_path).unwrap();
    let verificado = escrito.verificar_temporal().unwrap();

    let observacion = verificado.revalidar_completo().unwrap();

    assert_eq!(
        format!("{observacion:?}"),
        "ObservacionIntentTemporalCompleta { .. }"
    );
    assert!(!canonical_path.exists());
    assert_eq!(fs::read(&temporary_path).unwrap(), bytes_intent);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn revalidacion_completa_rechaza_mutaciones_de_journal_contexto_y_root() {
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
        let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let escrito = escribir_intent(&mut owner);
        let bytes_intent = fs::read(intent_paths(&fixture).1).unwrap();
        let id_intent = id_archivo(&intent_paths(&fixture).1);
        let verificado = escrito.verificar_temporal().unwrap();

        let mut root_canary = None;
        match cambio {
            Cambio::Journal => {
                let journal_original = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
                let mut journal_mutado = Journal::new(
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
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
                fixture.publicada.bytes_journal = journal_mutado.encode().unwrap();
                fs::write(
                    fixture.publicada.ubicacion.journal().as_path(),
                    &fixture.publicada.bytes_journal,
                )
                .unwrap();
            }
            Cambio::Contexto => {
                let path = contexto_path(&fixture.publicada.ubicacion);
                let mut bytes = fs::read(&path).unwrap();
                bytes[0] ^= 1;
                fs::write(&path, &bytes).unwrap();
                fixture.publicada.bytes_contexto = bytes;
            }
            Cambio::Root => {
                let entrada = fixture
                    .publicada
                    .entradas
                    .iter()
                    .find(|entrada| {
                        entrada.tipo() == crate::transaccion::snapshot::TipoEntradaSnapshot::Archivo
                    })
                    .unwrap();
                let path = fixture.publicada.root.join(entrada.ruta());
                let mut bytes = fs::read(&path).unwrap();
                bytes.push(0xa5);
                fs::write(&path, &bytes).unwrap();
                root_canary = Some((path, bytes, identidad_directorio(&fixture.publicada.root)));
            }
        }

        let ids_storage = capturar_storage(&fixture);

        assert_rechazo(verificado.revalidar_completo());

        if let Some((path_root, bytes_root, id_root)) = root_canary {
            assert_eq!(fs::read(path_root).unwrap(), bytes_root);
            assert!(identidad_directorio(&fixture.publicada.root) == id_root);
            let path_contexto = contexto_path(&fixture.publicada.ubicacion);
            let path_journal = fixture
                .publicada
                .ubicacion
                .journal()
                .as_path()
                .to_path_buf();
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
        } else {
            verificar_storage(&fixture, ids_storage);
        }

        let temporary_path = intent_paths(&fixture).1;
        assert_eq!(fs::read(&temporary_path).unwrap(), bytes_intent);
        assert!(id_archivo(&temporary_path) == id_intent);
    }
}

#[test]
fn revalidacion_completa_rechaza_siblings_conflictivos_y_preserva_canarios() {
    #[derive(Clone, Copy)]
    enum Slot {
        JournalTemporal,
        ContextoTemporal,
        IntentCanonico,
    }

    for slot in [
        Slot::JournalTemporal,
        Slot::ContextoTemporal,
        Slot::IntentCanonico,
    ] {
        let temp = tempdir().unwrap();
        let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
        avanzar_a(&mut fixture, JournalState::Committed);
        let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let verificado = escribir_intent(&mut owner).verificar_temporal().unwrap();
        let (canonical_intent, temporary_intent) = intent_paths(&fixture);
        let parent = fixture.publicada.ubicacion.ruta().parent().unwrap();
        let blocker = match slot {
            Slot::JournalTemporal => {
                parent.join(fixture.publicada.ubicacion.nombre_journal_temporal())
            }
            Slot::ContextoTemporal => {
                let canonical = contexto_path(&fixture.publicada.ubicacion);
                let temporal = nombre_temporal_sibling(canonical.file_name().unwrap()).unwrap();
                canonical.with_file_name(temporal)
            }
            Slot::IntentCanonico => canonical_intent,
        };
        let canary = b"reserved sibling blocker";
        fs::write(&blocker, canary).unwrap();
        let id_blocker = id_archivo(&blocker);
        let bytes_intent = fs::read(&temporary_intent).unwrap();
        let id_intent = id_archivo(&temporary_intent);
        let ids_storage = capturar_storage(&fixture);

        assert_rechazo(verificado.revalidar_completo());

        verificar_storage(&fixture, ids_storage);
        assert_eq!(fs::read(&blocker).unwrap(), canary);
        assert!(id_archivo(&blocker) == id_blocker);
        assert_eq!(fs::read(&temporary_intent).unwrap(), bytes_intent);
        assert!(id_archivo(&temporary_intent) == id_intent);
    }
}

#[test]
fn revalidacion_completa_rechaza_backup_old_y_preserva_su_arbol() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), true);
    avanzar_a(&mut fixture, JournalState::Committed);
    let backup = fixture.publicada.ubicacion.backup().as_path().to_path_buf();
    let backup_retenido = temp.path().join("backup-old-retained");
    let id_backup = fixture.identidad_backup_old.as_ref().unwrap().clone();
    fs::rename(&backup, &backup_retenido).unwrap();
    assert!(identidad_directorio(&backup_retenido) == id_backup);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let verificado = escribir_intent(&mut owner).verificar_temporal().unwrap();

    if let Err(error) = fs::rename(&backup_retenido, &backup) {
        #[cfg(windows)]
        {
            assert!(matches!(
                error.raw_os_error(),
                Some(5) | Some(32) | Some(33)
            ));
            assert!(identidad_directorio(&backup_retenido) == id_backup);
            drop(verificado);
            drop(owner);
            drop(lock);
            verificar_arbol(&backup_retenido, fixture.entradas_old.as_ref().unwrap());
            eprintln!(
                "Windows bloqueó restaurar el backup Old bajo el handle; el rechazo de inventario no se ejercitó."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo restaurar el backup Old: {error}");
    }

    let ids_storage = capturar_storage(&fixture);
    let (canonical_intent, temporary_intent) = intent_paths(&fixture);
    let bytes_intent = fs::read(&temporary_intent).unwrap();
    let id_intent = id_archivo(&temporary_intent);

    assert_rechazo(verificado.revalidar_completo());

    verificar_storage(&fixture, ids_storage);
    assert!(identidad_directorio(&backup) == id_backup);
    verificar_arbol(&backup, fixture.entradas_old.as_ref().unwrap());
    assert!(!canonical_intent.exists());
    assert_eq!(fs::read(&temporary_intent).unwrap(), bytes_intent);
    assert!(id_archivo(&temporary_intent) == id_intent);
}

#[test]
fn revalidacion_completa_rechaza_stage_presente_y_preserva_arbol() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let verificado = escribir_intent(&mut owner).verificar_temporal().unwrap();
    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let stage = fixture
        .publicada
        .ubicacion
        .stage(journal.transaction_id())
        .unwrap()
        .as_path()
        .to_path_buf();
    fs::create_dir(&stage).unwrap();
    materializar(&stage, &fixture.publicada.entradas);
    let id_stage = identidad_directorio(&stage);
    let ids_storage = capturar_storage(&fixture);
    let (canonical_intent, temporary_intent) = intent_paths(&fixture);
    let bytes_intent = fs::read(&temporary_intent).unwrap();
    let id_intent = id_archivo(&temporary_intent);

    assert_rechazo(verificado.revalidar_completo());

    verificar_storage(&fixture, ids_storage);
    assert!(identidad_directorio(&stage) == id_stage);
    verificar_arbol(&stage, &fixture.publicada.entradas);
    assert!(!canonical_intent.exists());
    assert_eq!(fs::read(&temporary_intent).unwrap(), bytes_intent);
    assert!(id_archivo(&temporary_intent) == id_intent);
}

#[test]
fn revalidacion_completa_rechaza_reemplazo_mismo_frame_con_id_distinto() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let verificado = escribir_intent(&mut owner).verificar_temporal().unwrap();
    let (_, temporary_intent) = intent_paths(&fixture);
    let original_retenido = temporary_intent.with_file_name("intent-original-retained");
    let bytes_originales = fs::read(&temporary_intent).unwrap();
    let id_original = id_archivo(&temporary_intent);

    if let Err(error) = fs::rename(&temporary_intent, &original_retenido) {
        #[cfg(windows)]
        {
            assert!(matches!(
                error.raw_os_error(),
                Some(5) | Some(32) | Some(33)
            ));
            drop(verificado);
            drop(owner);
            drop(lock);
            assert_eq!(fs::read(&temporary_intent).unwrap(), bytes_originales);
            assert!(id_archivo(&temporary_intent) == id_original);
            eprintln!("Windows bloqueó mover el intent abierto; no se ejercitó su reemplazo.");
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo retener el intent original: {error}");
    }

    fs::write(&temporary_intent, &bytes_originales).unwrap();
    let id_reemplazo = id_archivo(&temporary_intent);
    assert_ne!(id_reemplazo, id_original);
    let ids_storage = capturar_storage(&fixture);

    assert_rechazo(verificado.revalidar_completo());

    verificar_storage(&fixture, ids_storage);
    assert_eq!(fs::read(&original_retenido).unwrap(), bytes_originales);
    assert!(id_archivo(&original_retenido) == id_original);
    assert_eq!(fs::read(&temporary_intent).unwrap(), bytes_originales);
    assert!(id_archivo(&temporary_intent) == id_reemplazo);
}
