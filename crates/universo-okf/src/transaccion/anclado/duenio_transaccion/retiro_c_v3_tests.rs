use super::super::DuenioTransaccionAnclado;
use super::super::fixture_root_replay::{
    FixturePublicada, contexto_path, id_archivo, verificar_arbol,
};
use super::super::normal_v2_test_support::preparar_normal_v2;
use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
use crate::transaccion::anclado::intent_persistido_v2::IntentRetiroPersistidoV2;
use crate::transaccion::anclado::intent_persistido_v3::IntentRetiroPersistidoV3;
use crate::transaccion::anclado::scope_filesystem;
use crate::transaccion::journal::{JournalState, v2::JournalV2};
use crate::transaccion::lock::LockEscritura;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

fn fixture(parent: &Path) -> (FixturePublicada, PathBuf, Vec<u8>) {
    fixture_con_policy(parent, false)
}

fn fixture_con_policy(parent: &Path, unsupported: bool) -> (FixturePublicada, PathBuf, Vec<u8>) {
    let (mut fixture, _) = preparar_normal_v2(parent, JournalState::Committed);
    if unsupported {
        use crate::transaccion::admision::envelope::{
            tests::contexto_con_tags, v2::EnvelopeContextoAdmisionV2,
        };
        use crate::transaccion::replay::semantica_archivada::VERSION_MOTOR_V1;
        let context = contexto_con_tags(b"\n", VERSION_MOTOR_V1, "unsupported-private-policy");
        let journal = JournalV2::parse(&fixture.bytes_journal).unwrap();
        fixture.bytes_contexto = EnvelopeContextoAdmisionV2::ligar(context, &journal)
            .encode()
            .unwrap();
        fs::write(contexto_path(&fixture.ubicacion), &fixture.bytes_contexto).unwrap();
    }
    let intent = parent.join(".bundle.felix.admctx-retire");
    File::create(&intent).unwrap();
    let id = |path: &Path| IdentidadArchivoPersistida::try_from(id_archivo(path)).unwrap();
    let scope = scope_filesystem(&File::open(&intent).unwrap()).unwrap();
    let metadata = IntentRetiroPersistidoV2::desde_datos_persistidos(
        JournalV2::parse(&fixture.bytes_journal).unwrap(),
        id(&intent),
        id(fixture.ubicacion.journal().as_path()),
        id(&contexto_path(&fixture.ubicacion)),
        scope.0,
        fixture.bytes_contexto.len() as u64,
        Sha256::digest(&fixture.bytes_contexto).into(),
    )
    .unwrap();
    let frame = IntentRetiroPersistidoV3::encode(&metadata, &fixture.bytes_contexto).unwrap();
    fs::write(&intent, &frame).unwrap();
    fs::remove_file(fixture.ubicacion.journal().as_path()).unwrap();
    fs::remove_file(contexto_path(&fixture.ubicacion)).unwrap();
    (fixture, intent, frame)
}

#[test]
fn c_reproduce_contexto_sin_sidecars_y_no_muta_root_o_intent() {
    let dir = tempfile::tempdir().unwrap();
    let (fixture, intent, frame) = fixture(dir.path());
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let proof = owner.observar_retiro_c_v3_consistente().unwrap();
    drop(proof);
    assert_eq!(fs::read(intent).unwrap(), frame);
    verificar_arbol(&fixture.root, &fixture.entradas);
    assert!(!fixture.ubicacion.journal().as_path().exists());
    assert!(!contexto_path(&fixture.ubicacion).exists());
}

#[test]
fn root_modificado_o_reemplazado_no_obtiene_prueba() {
    for reemplazar in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, intent, frame) = fixture(dir.path());
        if reemplazar {
            fs::rename(&fixture.root, dir.path().join("original")).unwrap();
            fs::create_dir(&fixture.root).unwrap();
            super::super::fixture_root_replay::materializar(&fixture.root, &fixture.entradas);
        } else {
            fs::write(fixture.root.join("PRIVATE_NEW_FILE"), b"canary").unwrap();
        }
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        assert!(owner.observar_retiro_c_v3_consistente().is_err());
        assert_eq!(fs::read(intent).unwrap(), frame);
    }
}

#[test]
fn journal_context_backup_stage_y_temporal_presentes_se_preservan() {
    for slot in 0..5 {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, intent, frame) = fixture(dir.path());
        let journal = JournalV2::parse(&fixture.bytes_journal).unwrap();
        let path = match slot {
            0 => fixture.ubicacion.journal().as_path().to_path_buf(),
            1 => contexto_path(&fixture.ubicacion),
            2 => fixture.ubicacion.backup().as_path().to_path_buf(),
            3 => fixture
                .ubicacion
                .stage(journal.transaction_id())
                .unwrap()
                .as_path()
                .to_path_buf(),
            _ => dir.path().join(fixture.ubicacion.nombre_journal_temporal()),
        };
        fs::write(&path, b"PRIVATE_SIDECAR_CANARY").unwrap();
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let error = owner.observar_retiro_c_v3_consistente().err().unwrap();
        assert!(!format!("{error:?}").contains("PRIVATE"));
        assert_eq!(fs::read(&path).unwrap(), b"PRIVATE_SIDECAR_CANARY");
        assert_eq!(fs::read(intent).unwrap(), frame);
        verificar_arbol(&fixture.root, &fixture.entradas);
    }
}

#[test]
fn replay_policy_no_soportada_con_binding_valido_preserva_evidencia() {
    let dir = tempfile::tempdir().unwrap();
    let (fixture, intent, frame) = fixture_con_policy(dir.path(), true);
    assert!(IntentRetiroPersistidoV3::decode(frame.clone()).is_ok());
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    assert!(owner.observar_retiro_c_v3_consistente().is_err());
    assert_eq!(fs::read(intent).unwrap(), frame);
    verificar_arbol(&fixture.root, &fixture.entradas);
}
