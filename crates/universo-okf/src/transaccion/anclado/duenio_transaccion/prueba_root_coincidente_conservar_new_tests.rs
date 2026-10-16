use super::*;

use crate::transaccion::admision::envelope::{EnvelopeContextoAdmision, codec, tests as fixtures};
use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    FixturePublicada, assert_storage_unchanged, contexto_path, id_archivo, materializar,
    preparar_con_tags, verificar_arbol,
};
use crate::transaccion::anclado::{
    IdentidadArchivo, abrir_directorio_absoluto, abrir_directorio_hijo_lectura, metadata_directorio,
};
use crate::transaccion::inventario::{LimitesInventario, inventariar_root_anclado_con_limites};
use crate::transaccion::journal::recovery::{RecoveryDecision, RecoveryStep};
use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, Journal, JournalState};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::replay::new_esperado::calcular_new_esperado;
use crate::transaccion::replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};
use crate::transaccion::snapshot::{
    EntradaSnapshot, TipoEntradaSnapshot, calcular_digest_raw_tree,
};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

struct FixtureFilaConservarNew {
    publicada: FixturePublicada,
    entradas_old: Option<Vec<EntradaSnapshot>>,
    identidad_backup_old: Option<DirectoryIdentity>,
}

fn preparar_fixture(parent: &Path, con_backup_old: bool) -> FixtureFilaConservarNew {
    let mut publicada = preparar_con_tags(parent, VERSION_MOTOR_V1, POLICY_ID_V1);
    let mut entradas_old = None;
    let mut identidad_backup_old = None;
    if con_backup_old {
        let contexto = fixtures::contexto_con_tags(b"\n", VERSION_MOTOR_V1, POLICY_ID_V1);
        let base = contexto.entradas_base();
        let ruta_backup = publicada.ubicacion.backup().as_path().to_path_buf();
        fs::create_dir(&ruta_backup).unwrap();
        materializar(&ruta_backup, &base);

        let mut lock = LockEscritura::adquirir_en(&publicada.ubicacion).unwrap();
        let owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let nombre_backup = ruta_backup.file_name().unwrap();
        let ancla_backup = owner.abrir_sibling_lectura(nombre_backup).unwrap().unwrap();
        let identidad_backup = ancla_backup
            .identidad_raiz()
            .and_then(|identidad| identidad.journal_identity())
            .unwrap();
        let entradas_inventariadas =
            inventariar_root_anclado_con_limites(ancla_backup, LimitesInventario::CONTRATO)
                .unwrap();
        let digest_old = calcular_digest_raw_tree(&entradas_inventariadas).unwrap();
        let old = ArtifactFingerprint::new(identidad_backup.clone(), digest_old);
        let journal_previo = Journal::parse(&publicada.bytes_journal).unwrap();
        let journal = Journal::new(
            journal_previo.transaction_id(),
            journal_previo.parent().clone(),
            old.clone(),
            publicada.esperado.clone(),
        )
        .unwrap();
        let replay =
            calcular_new_esperado(&contexto, &journal, VERSION_MOTOR_V1, POLICY_ID_V1).unwrap();
        assert!(replay.old() == &old);
        assert!(replay.new_fingerprint() == &publicada.esperado);

        let envelope = EnvelopeContextoAdmision::ligar(contexto, &journal);
        let bytes_contexto = codec::codificar(&envelope).unwrap();
        let bytes_journal = journal.encode().unwrap();
        fs::write(contexto_path(&publicada.ubicacion), &bytes_contexto).unwrap();
        let ruta_journal = publicada.ubicacion.journal().as_path().to_path_buf();
        fs::write(&ruta_journal, &bytes_journal).unwrap();
        publicada.bytes_contexto = bytes_contexto;
        publicada.bytes_journal = bytes_journal;
        drop(owner);
        drop(lock);
        entradas_old = Some(entradas_inventariadas);
        identidad_backup_old = Some(identidad_backup);
    }

    FixtureFilaConservarNew {
        publicada,
        entradas_old,
        identidad_backup_old,
    }
}

fn identidad_directorio(path: &Path) -> DirectoryIdentity {
    let parent = abrir_directorio_absoluto(path.parent().unwrap(), true).unwrap();
    let directorio = abrir_directorio_hijo_lectura(&parent, path.file_name().unwrap()).unwrap();
    metadata_directorio(&directorio)
        .unwrap()
        .journal_identity()
        .unwrap()
}

fn avanzar_a(fixture: &mut FixtureFilaConservarNew, fase: JournalState) {
    let mut journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    for siguiente in [
        JournalState::BackupReady,
        JournalState::Installed,
        JournalState::Committed,
    ] {
        if journal.state() == fase {
            break;
        }
        journal.advance(siguiente).unwrap();
    }
    assert_eq!(journal.state(), fase);
    let bytes = journal.encode().unwrap();
    let ruta_journal = fixture
        .publicada
        .ubicacion
        .journal()
        .as_path()
        .to_path_buf();
    fs::write(ruta_journal, &bytes).unwrap();
    fixture.publicada.bytes_journal = bytes;
}

fn capturar_storage(fixture: &FixtureFilaConservarNew) -> (IdentidadArchivo, IdentidadArchivo) {
    let ruta_contexto = contexto_path(&fixture.publicada.ubicacion);
    let ruta_journal = fixture
        .publicada
        .ubicacion
        .journal()
        .as_path()
        .to_path_buf();
    (id_archivo(&ruta_contexto), id_archivo(&ruta_journal))
}

fn verificar_storage(fixture: &FixtureFilaConservarNew, ids: (IdentidadArchivo, IdentidadArchivo)) {
    assert_storage_unchanged(
        &fixture.publicada,
        &fixture.publicada.bytes_contexto,
        &fixture.publicada.bytes_journal,
        ids.0,
        ids.1,
    );
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}

fn rechazar_fila(fixture: &FixtureFilaConservarNew) {
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();

    let error = prueba.observar_fila_conservar_new().unwrap_err();

    assert_eq!(error.to_string(), "admission_replay_root_not_proven");
    assert_eq!(format!("{error:?}"), "ErrorPruebaReplayRootCoincidente");
}

#[test]
fn observa_fila_exacta_para_installed_y_committed_con_backup_ausente_o_old() {
    for fase in [JournalState::Installed, JournalState::Committed] {
        for con_backup_old in [false, true] {
            let temp = tempdir().unwrap();
            let mut fixture = preparar_fixture(&temp.path().join("parent"), con_backup_old);
            avanzar_a(&mut fixture, fase);
            let ids_storage = capturar_storage(&fixture);
            let id_root = identidad_directorio(&fixture.publicada.root);
            let ruta_backup = fixture.publicada.ubicacion.backup().as_path().to_path_buf();
            let id_backup = fixture.identidad_backup_old.clone().or_else(|| {
                ruta_backup
                    .exists()
                    .then(|| identidad_directorio(&ruta_backup))
            });

            let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
            let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
            let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();

            let observacion = prueba.observar_fila_conservar_new().unwrap();

            let (decision, pasos): (RecoveryDecision, &[RecoveryStep]) = match fase {
                JournalState::Installed => (
                    RecoveryDecision::KeepValidatedNewAndCommit,
                    &[
                        RecoveryStep::RemoveKnownStage,
                        RecoveryStep::RemoveKnownBackup,
                        RecoveryStep::PersistCommitted,
                        RecoveryStep::RemoveJournal,
                    ],
                ),
                JournalState::Committed => (
                    RecoveryDecision::KeepValidatedCommittedNew,
                    &[
                        RecoveryStep::RemoveKnownStage,
                        RecoveryStep::RemoveKnownBackup,
                        RecoveryStep::RemoveJournal,
                    ],
                ),
                _ => unreachable!(),
            };
            assert_eq!(observacion.decision, decision);
            assert_eq!(observacion.pasos, pasos);
            assert_eq!(observacion.prueba.journal.state(), fase);
            assert!(Arc::ptr_eq(
                &observacion.prueba._autoridad,
                &observacion.prueba._owner._autoridad
            ));
            assert_eq!(format!("{observacion:?}"), "ObservacionConservarNew { .. }");
            assert!(
                !format!("{observacion:?}").contains(observacion.prueba.journal.transaction_id())
            );
            verificar_storage(&fixture, ids_storage);
            assert!(identidad_directorio(&fixture.publicada.root) == id_root);
            match (&fixture.entradas_old, &id_backup) {
                (Some(entries), Some(identity)) => {
                    assert!(&identidad_directorio(&ruta_backup) == identity);
                    verificar_arbol(&ruta_backup, entries);
                }
                (None, None) => assert!(!ruta_backup.exists()),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn rechaza_stage_presente_y_preserva_cada_sibling() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Installed);
    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
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
    let ids_storage = capturar_storage(&fixture);
    let id_root = identidad_directorio(&fixture.publicada.root);

    rechazar_fila(&fixture);

    verificar_storage(&fixture, ids_storage);
    assert!(identidad_directorio(&fixture.publicada.root) == id_root);
    assert!(identidad_directorio(&ruta_stage) == id_stage);
    verificar_arbol(&ruta_stage, &fixture.publicada.entradas);
}

#[test]
fn rechaza_backup_partial_old_y_preserva_el_payload_observado() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), true);
    avanzar_a(&mut fixture, JournalState::Installed);
    let ruta_backup = fixture.publicada.ubicacion.backup().as_path().to_path_buf();
    let identidad_backup = fixture.identidad_backup_old.as_ref().unwrap().clone();
    let mut entradas_mutadas = fixture.entradas_old.clone().unwrap();
    let indice = entradas_mutadas
        .iter()
        .position(|entrada| entrada.tipo() == TipoEntradaSnapshot::Archivo)
        .unwrap();
    let bytes_originales = entradas_mutadas[indice].bytes().unwrap();
    let mut bytes_mutados = bytes_originales.to_vec();
    bytes_mutados[0] ^= 1;
    let ruta_archivo = ruta_backup.join(entradas_mutadas[indice].ruta());
    fs::write(&ruta_archivo, &bytes_mutados).unwrap();
    entradas_mutadas[indice] =
        EntradaSnapshot::archivo(entradas_mutadas[indice].ruta(), bytes_mutados.as_slice());
    let ids_storage = capturar_storage(&fixture);
    let id_root = identidad_directorio(&fixture.publicada.root);

    rechazar_fila(&fixture);

    verificar_storage(&fixture, ids_storage);
    assert!(identidad_directorio(&fixture.publicada.root) == id_root);
    assert!(identidad_directorio(&ruta_backup) == identidad_backup);
    verificar_arbol(&ruta_backup, &entradas_mutadas);
}

#[test]
fn rechaza_backup_unknown_y_preserva_original_y_decoy() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), true);
    avanzar_a(&mut fixture, JournalState::Installed);
    let ruta_backup = fixture.publicada.ubicacion.backup().as_path().to_path_buf();
    let ruta_original_retenido = temp.path().join("backup-original-retained");
    let id_original = fixture.identidad_backup_old.as_ref().unwrap().clone();
    fs::rename(&ruta_backup, &ruta_original_retenido).unwrap();
    assert!(identidad_directorio(&ruta_original_retenido) == id_original);
    let entradas_decoy = vec![EntradaSnapshot::archivo(
        "unknown.bin",
        &b"backup decoy"[..],
    )];
    materializar(&ruta_backup, &entradas_decoy);
    let id_decoy = identidad_directorio(&ruta_backup);
    assert!(id_decoy != id_original);
    let ids_storage = capturar_storage(&fixture);
    let id_root = identidad_directorio(&fixture.publicada.root);

    rechazar_fila(&fixture);

    verificar_storage(&fixture, ids_storage);
    assert!(identidad_directorio(&fixture.publicada.root) == id_root);
    assert!(identidad_directorio(&ruta_backup) == id_decoy);
    verificar_arbol(&ruta_backup, &entradas_decoy);
    assert!(identidad_directorio(&ruta_original_retenido) == id_original);
    verificar_arbol(
        &ruta_original_retenido,
        fixture.entradas_old.as_ref().unwrap(),
    );
}
