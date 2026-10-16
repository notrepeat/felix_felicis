use super::*;
use crate::transaccion::admision::envelope::{EnvelopeContextoAdmision, codec, tests as fixtures};
use sha2::{Digest, Sha256};

#[test]
fn revalidacion_sin_cambios_reemite_la_prueba_con_el_mismo_owner() {
    let temp = tempdir().unwrap();
    let fixture = preparar_soportado(&temp.path().join("parent"));
    fijar_estado(&fixture, JournalState::Installed);
    let storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();

    let prueba = prueba.revalidar().unwrap();

    assert!(Arc::ptr_eq(&prueba._autoridad, &prueba._owner._autoridad));
    assert!(prueba.identidad_parent == prueba._owner.identidad_parent);
    assert!(prueba.scope_parent == prueba._owner.scope_parent);
    verificar_storage(&fixture, &storage);
    verificar_arbol(&fixture.root, &fixture.entradas);
}

#[test]
fn journal_que_avanza_a_committed_invalida_la_prueba_anterior() {
    let temp = tempdir().unwrap();
    let fixture = preparar_soportado(&temp.path().join("parent"));
    fijar_estado(&fixture, JournalState::Installed);
    let storage = capturar_storage(&fixture);
    let path_journal = ruta_journal(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();

    let mut journal_nuevo = Journal::parse(&storage.bytes_journal).unwrap();
    journal_nuevo.advance(JournalState::Committed).unwrap();
    let bytes_journal_nuevo = journal_nuevo.encode().unwrap();
    fs::write(&path_journal, &bytes_journal_nuevo).unwrap();
    assert!(id_archivo(&path_journal) == storage.id_journal);
    assert_eq!(journal_nuevo.state(), JournalState::Committed);
    assert!(journal_nuevo.parent() == prueba.journal.parent());
    assert!(journal_nuevo.old() == prueba.journal.old());
    assert!(journal_nuevo.new_fingerprint() == prueba.journal.new_fingerprint());

    assert!(prueba.revalidar().is_err());

    assert_eq!(fs::read(&path_journal).unwrap(), bytes_journal_nuevo);
    assert!(id_archivo(&path_journal) == storage.id_journal);
    assert_eq!(
        fs::read(contexto_path(&fixture.ubicacion)).unwrap(),
        storage.bytes_contexto
    );
    assert!(id_archivo(&contexto_path(&fixture.ubicacion)) == storage.id_contexto);
    verificar_arbol(&fixture.root, &fixture.entradas);
}

#[test]
fn frame_valido_mismo_binding_y_new_pero_sha_distinto_invalida_prueba() {
    let temp = tempdir().unwrap();
    let fixture = preparar_soportado(&temp.path().join("parent"));
    fijar_estado(&fixture, JournalState::Installed);
    let storage = capturar_storage(&fixture);
    let path_contexto = contexto_path(&fixture.ubicacion);
    let journal = Journal::parse(&storage.bytes_journal).unwrap();
    let contexto_modificado = fixtures::contexto_con_tags(b" ", VERSION_MOTOR_V1, POLICY_ID_V1);
    let envelope_modificado = EnvelopeContextoAdmision::ligar(contexto_modificado, &journal);
    let replay_modificado = envelope_modificado
        .validar_replay_soportado(&journal)
        .unwrap();
    assert!(replay_modificado.new_fingerprint() == journal.new_fingerprint());
    let bytes_contexto_modificado = codec::codificar(&envelope_modificado).unwrap();
    assert_ne!(bytes_contexto_modificado, storage.bytes_contexto);
    assert_eq!(
        bytes_contexto_modificado.len(),
        storage.bytes_contexto.len()
    );
    let sha_contexto_original: [u8; 32] = Sha256::digest(&storage.bytes_contexto).into();
    let sha_contexto_modificado: [u8; 32] = Sha256::digest(&bytes_contexto_modificado).into();
    assert_ne!(sha_contexto_original, sha_contexto_modificado);

    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();
    assert_eq!(prueba.evidencia_contexto.sha256, sha_contexto_original);
    fs::write(&path_contexto, &bytes_contexto_modificado).unwrap();
    assert!(id_archivo(&path_contexto) == storage.id_contexto);
    assert_eq!(fs::read(&path_contexto).unwrap(), bytes_contexto_modificado);

    assert!(prueba.revalidar().is_err());

    assert_eq!(fs::read(&path_contexto).unwrap(), bytes_contexto_modificado);
    assert!(id_archivo(&path_contexto) == storage.id_contexto);
    assert_eq!(
        fs::read(ruta_journal(&fixture)).unwrap(),
        storage.bytes_journal
    );
    assert!(id_archivo(&ruta_journal(&fixture)) == storage.id_journal);
    verificar_arbol(&fixture.root, &fixture.entradas);
}

#[test]
fn reemplazar_journal_o_contexto_por_los_mismos_bytes_con_otro_id_invalida_prueba() {
    for reemplazar_contexto in [false, true] {
        let temp = tempdir().unwrap();
        let fixture = preparar_soportado(&temp.path().join("parent"));
        fijar_estado(&fixture, JournalState::Installed);
        let storage = capturar_storage(&fixture);
        let path_journal = ruta_journal(&fixture);
        let path_contexto = contexto_path(&fixture.ubicacion);
        let (
            path_canonico,
            bytes_originales,
            identidad_original,
            retained,
            path_inalterado,
            bytes_inalterados,
            identidad_inalterada,
        ) = if reemplazar_contexto {
            (
                &path_contexto,
                storage.bytes_contexto.as_slice(),
                &storage.id_contexto,
                path_contexto.with_file_name(".retained-context"),
                &path_journal,
                storage.bytes_journal.as_slice(),
                &storage.id_journal,
            )
        } else {
            (
                &path_journal,
                storage.bytes_journal.as_slice(),
                &storage.id_journal,
                path_journal.with_file_name(".retained-journal"),
                &path_contexto,
                storage.bytes_contexto.as_slice(),
                &storage.id_contexto,
            )
        };
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();

        fs::rename(path_canonico, &retained).unwrap();
        fs::write(path_canonico, bytes_originales).unwrap();
        assert_eq!(fs::read(path_canonico).unwrap(), bytes_originales);
        assert_eq!(fs::read(&retained).unwrap(), bytes_originales);
        assert!(id_archivo(&retained) == *identidad_original);
        assert!(id_archivo(path_canonico) != *identidad_original);
        assert_eq!(fs::read(path_inalterado).unwrap(), bytes_inalterados);
        assert!(id_archivo(path_inalterado) == *identidad_inalterada);

        assert!(prueba.revalidar().is_err());

        assert_eq!(fs::read(path_canonico).unwrap(), bytes_originales);
        assert!(id_archivo(path_canonico) != *identidad_original);
        assert_eq!(fs::read(&retained).unwrap(), bytes_originales);
        assert!(id_archivo(&retained) == *identidad_original);
        assert_eq!(fs::read(path_inalterado).unwrap(), bytes_inalterados);
        assert!(id_archivo(path_inalterado) == *identidad_inalterada);
        verificar_arbol(&fixture.root, &fixture.entradas);
    }
}
