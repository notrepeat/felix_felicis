use super::*;
use crate::transaccion::snapshot::TipoEntradaSnapshot;

#[test]
fn temporal_sibling_presente_bloquea_y_preserva_todos_los_canaries() {
    for contexto_temporal in [false, true] {
        let temp = tempdir().unwrap();
        let fixture = preparar_soportado(&temp.path().join("parent"));
        fijar_estado(&fixture, JournalState::Committed);
        let storage = capturar_storage(&fixture);
        let temporal = if contexto_temporal {
            let mut temporal = contexto_path(&fixture.ubicacion);
            let mut nombre = temporal.file_name().unwrap().to_os_string();
            nombre.push(".tmp");
            temporal.set_file_name(nombre);
            temporal
        } else {
            fixture
                .ubicacion
                .ruta()
                .parent()
                .unwrap()
                .join(fixture.ubicacion.nombre_journal_temporal())
        };
        let canary = b"orphan transaction temporary";
        fs::write(&temporal, canary).unwrap();
        let id_temporal = id_archivo(&temporal);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        assert!(owner.probar_replay_admitido_root_coincidente().is_err());

        verificar_storage(&fixture, &storage);
        assert_eq!(fs::read(&temporal).unwrap(), canary);
        assert!(id_archivo(&temporal) == id_temporal);
        verificar_arbol(&fixture.root, &fixture.entradas);
    }
}

#[test]
fn sibling_canonico_ausente_bloquea_sin_alterar_el_restante() {
    for falta_journal in [true, false] {
        let temp = tempdir().unwrap();
        let fixture = preparar_soportado(&temp.path().join("parent"));
        fijar_estado(&fixture, JournalState::Committed);
        let path_journal = ruta_journal(&fixture);
        let path_contexto = contexto_path(&fixture.ubicacion);
        let faltante = if falta_journal {
            &path_journal
        } else {
            &path_contexto
        };
        fs::remove_file(faltante).unwrap();
        let bytes_restante = if falta_journal {
            fs::read(&path_contexto).unwrap()
        } else {
            fs::read(&path_journal).unwrap()
        };
        let id_restante = if falta_journal {
            id_archivo(&path_contexto)
        } else {
            id_archivo(&path_journal)
        };
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        assert!(owner.probar_replay_admitido_root_coincidente().is_err());

        assert!(!faltante.exists());
        if falta_journal {
            assert_eq!(fs::read(&path_contexto).unwrap(), bytes_restante);
            assert!(id_archivo(&path_contexto) == id_restante);
        } else {
            assert_eq!(fs::read(&path_journal).unwrap(), bytes_restante);
            assert!(id_archivo(&path_journal) == id_restante);
        }
        verificar_arbol(&fixture.root, &fixture.entradas);
    }
}

#[test]
fn tags_no_soportados_no_emiten_prueba_y_preservan_storage() {
    let temp = tempdir().unwrap();
    let fixture = preparar_con_tags(
        &temp.path().join("parent"),
        "legacy-proposal/0",
        "legacy-policy/0",
    );
    fijar_estado(&fixture, JournalState::Installed);
    let storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    assert!(owner.probar_replay_admitido_root_coincidente().is_err());

    verificar_storage(&fixture, &storage);
    verificar_arbol(&fixture.root, &fixture.entradas);
}

#[test]
fn root_ausente_no_emite_prueba_y_preserva_original_y_siblings() {
    let temp = tempdir().unwrap();
    let fixture = preparar_soportado(&temp.path().join("parent"));
    fijar_estado(&fixture, JournalState::Installed);
    let storage = capturar_storage(&fixture);
    let original = temp.path().join("root-original-retained");
    fs::rename(&fixture.root, &original).unwrap();
    assert!(!fixture.root.exists());
    verificar_arbol(&original, &fixture.entradas);

    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    assert!(owner.probar_replay_admitido_root_coincidente().is_err());

    assert!(!fixture.root.exists());
    verificar_arbol(&original, &fixture.entradas);
    verificar_storage(&fixture, &storage);
}

#[test]
fn root_mismo_id_digest_distinto_no_emite_prueba_ni_altera_canaries() {
    let temp = tempdir().unwrap();
    let fixture = preparar_soportado(&temp.path().join("parent"));
    fijar_estado(&fixture, JournalState::Installed);
    let storage = capturar_storage(&fixture);
    let entrada = fixture
        .entradas
        .iter()
        .find(|entrada| entrada.tipo() == TipoEntradaSnapshot::Archivo)
        .unwrap();
    let ruta_archivo = fixture.root.join(entrada.ruta());
    let mut bytes_modificados = fs::read(&ruta_archivo).unwrap();
    bytes_modificados.push(b'!');
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let root_antes = owner.observar_root_raw().unwrap().unwrap();
    fs::write(&ruta_archivo, &bytes_modificados).unwrap();
    let root_modificado = owner.observar_root_raw().unwrap().unwrap();
    assert!(root_modificado.identity() == root_antes.identity());
    assert!(root_modificado != fixture.esperado);

    assert!(owner.probar_replay_admitido_root_coincidente().is_err());

    assert!(owner.observar_root_raw().unwrap().unwrap() == root_modificado);
    for entrada_actual in &fixture.entradas {
        let ruta = fixture.root.join(entrada_actual.ruta());
        match entrada_actual.tipo() {
            TipoEntradaSnapshot::Directorio => assert!(ruta.is_dir()),
            TipoEntradaSnapshot::Archivo if entrada_actual.ruta() == entrada.ruta() => {
                assert_eq!(fs::read(ruta).unwrap(), bytes_modificados);
            }
            TipoEntradaSnapshot::Archivo => {
                assert_eq!(fs::read(ruta).unwrap(), entrada_actual.bytes().unwrap());
            }
        }
    }
    verificar_storage(&fixture, &storage);
}
