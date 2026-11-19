use super::*;
use crate::transaccion::admision::envelope::tests as fixtures;
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    FixturePublicada, assert_storage_unchanged, contexto_path, id_archivo, materializar, preparar,
    preparar_con_tags, verificar_arbol,
};
use crate::transaccion::journal::{DirectoryIdentity, Journal};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};
use crate::transaccion::snapshot::TipoEntradaSnapshot;
use std::fs;
use tempfile::tempdir;

struct StorageSnapshot {
    bytes_contexto: Vec<u8>,
    bytes_journal: Vec<u8>,
    id_contexto: crate::transaccion::anclado::IdentidadArchivo,
    id_journal: crate::transaccion::anclado::IdentidadArchivo,
}

fn capturar_storage(fixture: &FixturePublicada) -> StorageSnapshot {
    let path_contexto = contexto_path(&fixture.ubicacion);
    let path_journal = fixture.ubicacion.journal().as_path().to_path_buf();
    StorageSnapshot {
        bytes_contexto: fs::read(&path_contexto).unwrap(),
        bytes_journal: fs::read(&path_journal).unwrap(),
        id_contexto: id_archivo(&path_contexto),
        id_journal: id_archivo(&path_journal),
    }
}

fn verificar_storage(fixture: &FixturePublicada, antes: &StorageSnapshot) {
    assert_storage_unchanged(
        fixture,
        &antes.bytes_contexto,
        &antes.bytes_journal,
        antes.id_contexto,
        antes.id_journal,
    );
}

fn identidad_parent_distinta() -> DirectoryIdentity {
    #[cfg(unix)]
    {
        DirectoryIdentity::unix(u64::MAX, u64::MAX)
    }
    #[cfg(windows)]
    {
        DirectoryIdentity::windows(u64::MAX, "ffffffffffffffffffffffffffffffff")
    }
}

#[test]
fn compara_identidad_y_digest_raw_y_devuelve_datos() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let coincide = owner
        .comparar_root_replay(fixtures::MOTOR, fixtures::POLICY)
        .unwrap();
    assert_eq!(coincide.relacion(), RelacionRootReplay::Coincide);
    assert!(coincide.esperado() == &fixture.esperado);
    assert!(coincide.observado() == Some(&fixture.esperado));

    let entrada_archivo = fixture
        .entradas
        .iter()
        .find(|entrada| entrada.tipo() == TipoEntradaSnapshot::Archivo)
        .unwrap();
    let path_archivo = fixture.root.join(entrada_archivo.ruta());
    let bytes_originales = fs::read(&path_archivo).unwrap();
    let mut bytes_cambiados = bytes_originales.clone();
    bytes_cambiados.push(b'!');
    fs::write(&path_archivo, &bytes_cambiados).unwrap();
    let digest_distinto = owner
        .comparar_root_replay(fixtures::MOTOR, fixtures::POLICY)
        .unwrap();
    assert_eq!(digest_distinto.relacion(), RelacionRootReplay::Diferente);
    assert!(digest_distinto.observado().unwrap().identity() == fixture.esperado.identity());
    assert!(digest_distinto.observado().unwrap() != &fixture.esperado);
    fs::write(&path_archivo, &bytes_originales).unwrap();

    let root_original = temp.path().join("root-original-retained");
    fs::rename(&fixture.root, &root_original).unwrap();
    fs::create_dir(&fixture.root).unwrap();
    materializar(&fixture.root, &fixture.entradas);
    verificar_arbol(&fixture.root, &fixture.entradas);
    let id_distinto = owner
        .comparar_root_replay(fixtures::MOTOR, fixtures::POLICY)
        .unwrap();
    assert_eq!(id_distinto.relacion(), RelacionRootReplay::Diferente);
    assert!(id_distinto.observado().unwrap().identity() != fixture.esperado.identity());
    assert!(id_distinto.observado().unwrap() != &fixture.esperado);

    let root_recreated = temp.path().join("root-recreated-retained");
    fs::rename(&fixture.root, root_recreated).unwrap();
    let ausente = owner
        .comparar_root_replay(fixtures::MOTOR, fixtures::POLICY)
        .unwrap();
    assert_eq!(ausente.relacion(), RelacionRootReplay::Ausente);
    assert!(ausente.observado().is_none());
    verificar_storage(&fixture, &storage);
}

#[test]
fn journal_incompatible_o_parent_distinto_falla_y_preserva_siblings() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let path_journal = fixture.ubicacion.journal().as_path().to_path_buf();
    let journal = Journal::parse(&fixture.bytes_journal).unwrap();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    for (motor, policy) in [
        ("engine-unknown/1", fixtures::POLICY),
        (fixtures::MOTOR, "policy-unknown/1"),
    ] {
        let antes = capturar_storage(&fixture);
        let error = owner.comparar_root_replay(motor, policy).unwrap_err();
        assert_eq!(error.codigo(), "replay_engine_policy_mismatch");
        assert!(!format!("{error:?}").contains(temp.path().to_string_lossy().as_ref()));
        verificar_storage(&fixture, &antes);
    }

    let journal_otro_tx = Journal::new(
        "1123456789abcdef0123456789abcdef",
        journal.parent().clone(),
        journal.old().clone(),
        journal.new_fingerprint().clone(),
    )
    .unwrap();
    fs::write(&path_journal, journal_otro_tx.encode().unwrap()).unwrap();
    let antes = capturar_storage(&fixture);
    let error = owner
        .comparar_root_replay(fixtures::MOTOR, fixtures::POLICY)
        .unwrap_err();
    assert_eq!(error.codigo(), "admission_envelope_journal_mismatch");
    verificar_storage(&fixture, &antes);

    let journal_parent_distinto = Journal::new(
        journal.transaction_id(),
        identidad_parent_distinta(),
        journal.old().clone(),
        journal.new_fingerprint().clone(),
    )
    .unwrap();
    fs::write(&path_journal, journal_parent_distinto.encode().unwrap()).unwrap();
    let antes = capturar_storage(&fixture);
    let error = owner
        .comparar_root_replay(fixtures::MOTOR, fixtures::POLICY)
        .unwrap_err();
    assert_eq!(error.codigo(), "journal_parent_identity_mismatch");
    verificar_storage(&fixture, &antes);
}

#[test]
fn siblings_ausentes_fallan_cerrado_sin_tocar_el_que_permanece() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let path_contexto = contexto_path(&fixture.ubicacion);
    let path_journal = fixture.ubicacion.journal().as_path().to_path_buf();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let bytes_contexto = fs::read(&path_contexto).unwrap();
    let id_contexto = id_archivo(&path_contexto);
    fs::remove_file(&path_journal).unwrap();
    let error = owner
        .comparar_root_replay(fixtures::MOTOR, fixtures::POLICY)
        .unwrap_err();
    assert_eq!(error.codigo(), "root_replay_journal_missing");
    assert!(!path_journal.exists());
    assert_eq!(fs::read(&path_contexto).unwrap(), bytes_contexto);
    assert!(id_archivo(&path_contexto) == id_contexto);

    fs::write(&path_journal, &fixture.bytes_journal).unwrap();
    let bytes_journal = fs::read(&path_journal).unwrap();
    let id_journal = id_archivo(&path_journal);
    fs::remove_file(&path_contexto).unwrap();
    let error = owner
        .comparar_root_replay(fixtures::MOTOR, fixtures::POLICY)
        .unwrap_err();
    assert_eq!(error.codigo(), "root_replay_context_missing");
    assert!(!path_contexto.exists());
    assert_eq!(fs::read(&path_journal).unwrap(), bytes_journal);
    assert!(id_archivo(&path_journal) == id_journal);
}

#[test]
fn compara_replay_soportado_y_detecta_root_recreado_con_mismo_digest() {
    let temp = tempdir().unwrap();
    let fixture = preparar_con_tags(&temp.path().join("parent"), VERSION_MOTOR_V1, POLICY_ID_V1);
    let storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let coincide = owner.comparar_root_replay_soportado().unwrap();

    assert_eq!(coincide.relacion(), RelacionRootReplay::Coincide);
    assert!(coincide.esperado() == &fixture.esperado);
    assert!(coincide.observado() == Some(&fixture.esperado));

    let root_original = temp.path().join("root-original-retained");
    fs::rename(&fixture.root, &root_original).unwrap();
    fs::create_dir(&fixture.root).unwrap();
    materializar(&fixture.root, &fixture.entradas);
    verificar_arbol(&fixture.root, &fixture.entradas);

    let root_sustituido = owner.comparar_root_replay_soportado().unwrap();

    assert_eq!(root_sustituido.relacion(), RelacionRootReplay::Diferente);
    assert!(root_sustituido.observado().unwrap().identity() != fixture.esperado.identity());
    assert!(root_sustituido.observado().unwrap() != &fixture.esperado);
    assert!(root_original.is_dir());
    verificar_storage(&fixture, &storage);
}

#[test]
fn replay_soportado_rechaza_tags_anteriores_y_desconocidos_sin_mutar_storage() {
    let perfiles = [
        (fixtures::MOTOR, fixtures::POLICY),
        ("felix_felicis:proposal-semantics/v-next", POLICY_ID_V1),
        (VERSION_MOTOR_V1, "felix_felicis:admission-policy/v-next"),
    ];

    for (motor, policy) in perfiles {
        let temp = tempdir().unwrap();
        let fixture = preparar_con_tags(&temp.path().join("parent"), motor, policy);
        let storage = capturar_storage(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let error = owner.comparar_root_replay_soportado().unwrap_err();

        assert_eq!(error.codigo(), "replay_engine_policy_mismatch");
        assert!(!format!("{error:?}").contains(temp.path().to_string_lossy().as_ref()));
        drop(owner);
        drop(lock);
        verificar_storage(&fixture, &storage);
    }
}
