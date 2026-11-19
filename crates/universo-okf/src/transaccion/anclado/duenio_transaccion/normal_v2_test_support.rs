//! Fixture V2 compartida por consumidores de observación y replay.

use crate::transaccion::admision::envelope::{tests as fixtures, v2::EnvelopeContextoAdmisionV2};
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    FixturePublicada, contexto_path, id_archivo, preparar_con_tags,
};
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::journal::{ArtifactFingerprint, Journal, JournalState, RawTreeDigest};
use crate::transaccion::replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};
use std::fs;
use std::path::{Path, PathBuf};

const CANARY: &[u8] = b"unrelated sibling remains unchanged";

pub(super) struct StorageCapturada {
    contexto: Vec<u8>,
    journal: Vec<u8>,
    identidad_contexto: crate::transaccion::anclado::IdentidadArchivo,
    identidad_journal: crate::transaccion::anclado::IdentidadArchivo,
}

/// Adapta el fixture J1 compartido a J2 y ContextoV2 con tags elegidos.
///
/// El digest solo existe cuando el perfil cerrado admite esos tags; el estado
/// se avanza de forma tipada para cubrir los consumidores que limitan fases.
pub(super) fn preparar_normal_v2(
    parent: &Path,
    state: JournalState,
) -> (FixturePublicada, Option<RawTreeDigest>) {
    preparar_normal_v2_con_tags(parent, state, VERSION_MOTOR_V1, POLICY_ID_V1)
}

pub(super) fn preparar_normal_v2_con_tags(
    parent: &Path,
    state: JournalState,
    motor: &str,
    policy: &str,
) -> (FixturePublicada, Option<RawTreeDigest>) {
    let mut fixture = preparar_con_tags(parent, motor, policy);
    let journal_v1 = Journal::parse(&fixture.bytes_journal).unwrap();
    let mut journal_v2 = JournalV2::new(
        journal_v1.transaction_id(),
        journal_v1.parent().clone(),
        journal_v1.old().clone(),
        journal_v1.new_fingerprint().clone(),
    )
    .unwrap();
    let pasos = match state {
        JournalState::Prepared => 0,
        JournalState::BackupReady => 1,
        JournalState::Installed => 2,
        JournalState::Committed => 3,
    };
    for next in [
        JournalState::BackupReady,
        JournalState::Installed,
        JournalState::Committed,
    ]
    .into_iter()
    .take(pasos)
    {
        journal_v2.advance(next).unwrap();
    }

    let envelope = EnvelopeContextoAdmisionV2::ligar(
        fixtures::contexto_con_tags(b"\n", motor, policy),
        &journal_v2,
    );
    let digest = envelope
        .calcular_new_esperado(&journal_v2)
        .ok()
        .map(|new| new.digest().clone());
    if let Some(digest) = &digest {
        let esperado =
            ArtifactFingerprint::new(fixture.esperado.identity().clone(), digest.clone());
        assert!(esperado == fixture.esperado);
    }

    fixture.bytes_journal = journal_v2.encode().unwrap();
    fixture.bytes_contexto = envelope.encode().unwrap();
    fs::write(
        fixture.ubicacion.journal().as_path(),
        &fixture.bytes_journal,
    )
    .unwrap();
    fs::write(contexto_path(&fixture.ubicacion), &fixture.bytes_contexto).unwrap();
    (fixture, digest)
}

pub(super) fn capturar_storage(fixture: &FixturePublicada) -> StorageCapturada {
    let contexto = contexto_path(&fixture.ubicacion);
    let journal = fixture.ubicacion.journal().as_path().to_path_buf();
    StorageCapturada {
        contexto: fs::read(&contexto).unwrap(),
        journal: fs::read(&journal).unwrap(),
        identidad_contexto: id_archivo(&contexto),
        identidad_journal: id_archivo(&journal),
    }
}

pub(super) fn verificar_storage(fixture: &FixturePublicada, storage: &StorageCapturada) {
    crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::assert_storage_unchanged(
        fixture,
        &storage.contexto,
        &storage.journal,
        storage.identidad_contexto,
        storage.identidad_journal,
    );
}

pub(super) fn crear_canary(
    fixture: &FixturePublicada,
) -> (PathBuf, crate::transaccion::anclado::IdentidadArchivo) {
    let path = fixture.root.parent().unwrap().join("unrelated-canary");
    fs::write(&path, CANARY).unwrap();
    (path.clone(), id_archivo(&path))
}

pub(super) fn verificar_canary(
    path: &Path,
    identidad: crate::transaccion::anclado::IdentidadArchivo,
) {
    assert_eq!(fs::read(path).unwrap(), CANARY);
    assert!(id_archivo(path) == identidad);
}
