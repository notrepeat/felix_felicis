//! Fixtures reales para observar el Journal canónico reteniendo su handle.

use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::anclado::journal_temporal::versionado::ObservacionJournalPersistido;
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::journal::{Journal, JournalState};
use crate::transaccion::lock::LockEscritura;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn avanzar_v2_a(journal: &mut JournalV2, estado: JournalState) {
    for siguiente in [
        JournalState::BackupReady,
        JournalState::Installed,
        JournalState::Committed,
    ] {
        if journal.state() == estado {
            break;
        }
        journal.advance(siguiente).unwrap();
    }
    assert_eq!(journal.state(), estado);
}

pub(super) fn observar_v2_en_fase(
    root: &Path,
    lock: &mut LockEscritura,
    transaction_id: &str,
    estado: JournalState,
) -> (
    JournalV2,
    Vec<u8>,
    super::IdentidadArchivoJournal,
    ObservacionJournalPersistido,
) {
    let mut owner = DuenioTransaccionAnclado::desde_lock(lock).unwrap();
    let resultado = {
        let lease = owner.journal_temporal_estricto().unwrap();
        let mut journal = super::journal_v2_prueba(&lease, transaction_id);
        avanzar_v2_a(&mut journal, estado);
        let bytes = journal.encode().unwrap();
        let canonic = super::nombre_journal(root);
        fs::write(&canonic, &bytes).unwrap();
        let identidad = super::identidad(&canonic);
        let observacion = lease.observar_versionado().unwrap().unwrap();
        (journal, bytes, identidad, observacion)
    };
    drop(owner);
    resultado
}

pub(super) fn observar_v1(
    root: &Path,
    lock: &mut LockEscritura,
) -> (
    Journal,
    Vec<u8>,
    super::IdentidadArchivoJournal,
    ObservacionJournalPersistido,
) {
    let mut owner = DuenioTransaccionAnclado::desde_lock(lock).unwrap();
    let resultado = {
        let lease = owner.journal_temporal_estricto().unwrap();
        let journal = super::journal_prueba(&lease, "0123456789abcdef0123456789abcdef");
        let bytes = journal.encode().unwrap();
        let canonic = super::nombre_journal(root);
        fs::write(&canonic, &bytes).unwrap();
        let identidad = super::identidad(&canonic);
        let observacion = lease.observar_versionado().unwrap().unwrap();
        (journal, bytes, identidad, observacion)
    };
    drop(owner);
    resultado
}

pub(super) fn crear_canario(root: &Path) -> PathBuf {
    let canario = root.join("canario-preservado");
    fs::write(&canario, b"canario de raiz").unwrap();
    canario
}

pub(super) fn comprobar_canario(canario: &Path) {
    assert_eq!(fs::read(canario).unwrap(), b"canario de raiz");
    assert!(canario.parent().unwrap().is_dir());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::journal::selector::JournalPersistido;
    use tempfile::tempdir;

    #[test]
    fn observaciones_reales_preservan_v1_y_todas_las_fases_v2() {
        for (indice, estado) in [
            JournalState::Prepared,
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ]
        .into_iter()
        .enumerate()
        {
            let temp = tempdir().unwrap();
            let root = super::super::crear_root(temp.path());
            let canario = crear_canario(&root);
            let mut lock = LockEscritura::adquirir(&root).unwrap();
            let transaction_id = format!("{indice:032x}");
            let (journal, bytes, identidad, observacion) =
                observar_v2_en_fase(&root, &mut lock, &transaction_id, estado);
            let canonic = super::super::nombre_journal(&root);
            let temporal = super::super::nombre_temporal(temp.path());

            assert_eq!(journal.state(), estado);
            assert_eq!(observacion.identidad(), identidad);
            assert!(matches!(
                observacion.journal(),
                JournalPersistido::V2(actual) if actual == &journal
            ));
            assert_eq!(fs::read(&canonic).unwrap(), bytes);
            drop(observacion);
            drop(lock);

            comprobar_canario(&canario);
            assert_eq!(super::super::identidad(&canonic), identidad);
            assert_eq!(fs::read(canonic).unwrap(), bytes);
            assert!(!temporal.exists());
        }

        let temp = tempdir().unwrap();
        let root = super::super::crear_root(temp.path());
        let canario = crear_canario(&root);
        let mut lock = LockEscritura::adquirir(&root).unwrap();
        let (journal, bytes, identidad, observacion) = observar_v1(&root, &mut lock);
        let canonic = super::super::nombre_journal(&root);
        let temporal = super::super::nombre_temporal(temp.path());

        assert_eq!(observacion.identidad(), identidad);
        assert!(matches!(
            observacion.journal(),
            JournalPersistido::V1(actual) if actual == &journal
        ));
        assert_eq!(fs::read(&canonic).unwrap(), bytes);
        drop(observacion);
        drop(lock);

        comprobar_canario(&canario);
        assert_eq!(super::super::identidad(&canonic), identidad);
        assert_eq!(fs::read(canonic).unwrap(), bytes);
        assert!(!temporal.exists());
    }
}
