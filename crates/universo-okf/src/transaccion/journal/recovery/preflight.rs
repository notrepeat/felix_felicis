//! Journal and inventory snapshot collected under the writer lock.

use crate::transaccion::anclado::journal_temporal::{ErrorJournalAnclado, IdentidadArchivoJournal};
use crate::transaccion::journal::{Journal, JournalState};
use crate::transaccion::lock::LockEscritura;

use super::{
    ArtifactState, ErrorObservadorRecovery, RecoveryDecision, RecoveryInventory, RecoveryStep,
    decide_recovery, observar_inventario_recovery,
};

/// One of the only recovery rows currently implemented by a runner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FilaRecoverySoportada {
    KeepOld,
    RestoreOld,
}

/// Canonical journal identity and its inventory, observed while holding the same lock.
pub(crate) struct ContextoPreflightRecovery {
    pub(crate) identidad_journal: IdentidadArchivoJournal,
    pub(crate) journal: Journal,
    pub(crate) inventario: RecoveryInventory,
}

/// Missing is distinct from a present journal whose row is unsupported.
pub(crate) enum ResultadoPreflightRecovery {
    JournalAusente,
    FaseNoSoportada,
    Contexto(Box<ContextoPreflightRecovery>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorPreflightRecovery {
    Journal(ErrorJournalAnclado),
    Observacion(ErrorObservadorRecovery),
}

/// Reads the canonical journal first, then observes exactly one inventory under the same lock.
pub(crate) fn preflight_recovery(
    lock: &mut LockEscritura,
) -> Result<ResultadoPreflightRecovery, ErrorPreflightRecovery> {
    preflight_recovery_con_observador(lock, observar_inventario_recovery)
}

pub(super) fn preflight_recovery_con_observador(
    lock: &mut LockEscritura,
    observar: impl FnOnce(
        &LockEscritura,
        &Journal,
    ) -> Result<RecoveryInventory, ErrorObservadorRecovery>,
) -> Result<ResultadoPreflightRecovery, ErrorPreflightRecovery> {
    let journal_canonico = {
        let temporal = lock
            .journal_temporal_anclado()
            .map_err(ErrorPreflightRecovery::Journal)?;
        temporal
            .cargar_canonico()
            .map_err(ErrorPreflightRecovery::Journal)?
    };

    preflight_desde_journal_canonico(journal_canonico, |journal| observar(lock, journal))
}

fn preflight_desde_journal_canonico(
    journal_canonico: Option<(IdentidadArchivoJournal, Journal)>,
    observar: impl FnOnce(&Journal) -> Result<RecoveryInventory, ErrorObservadorRecovery>,
) -> Result<ResultadoPreflightRecovery, ErrorPreflightRecovery> {
    let Some((identidad_journal, journal)) = journal_canonico else {
        return Ok(ResultadoPreflightRecovery::JournalAusente);
    };
    let Some(inventario) = observar_si_fase_soportada(&journal, observar)? else {
        return Ok(ResultadoPreflightRecovery::FaseNoSoportada);
    };
    Ok(ResultadoPreflightRecovery::Contexto(Box::new(
        ContextoPreflightRecovery {
            identidad_journal,
            journal,
            inventario,
        },
    )))
}

fn observar_si_fase_soportada(
    journal: &Journal,
    observar: impl FnOnce(&Journal) -> Result<RecoveryInventory, ErrorObservadorRecovery>,
) -> Result<Option<RecoveryInventory>, ErrorPreflightRecovery> {
    if !fase_soportada(journal.state()) {
        return Ok(None);
    }
    observar(journal)
        .map(Some)
        .map_err(ErrorPreflightRecovery::Observacion)
}

fn fase_soportada(state: JournalState) -> bool {
    matches!(
        state,
        JournalState::Prepared | JournalState::BackupReady | JournalState::Installed
    )
}

/// Selects only the exact KeepOld and RestoreOld rows consumed by current runners.
/// Every other phase, inventory, decision, or recipe remains unsupported.
pub(crate) fn clasificar_fila_recovery(
    journal: &Journal,
    inventario: RecoveryInventory,
) -> Option<FilaRecoverySoportada> {
    if !fase_soportada(journal.state()) {
        return None;
    }

    let decision = decide_recovery(journal.state(), inventario);
    match decision {
        RecoveryDecision::KeepOld
            if inventario
                == (RecoveryInventory {
                    root: ArtifactState::Old,
                    backup: ArtifactState::Absent,
                    stage: ArtifactState::Absent,
                    root_new_validated: false,
                })
                && decision.steps(inventario) == [RecoveryStep::RemoveJournal] =>
        {
            Some(FilaRecoverySoportada::KeepOld)
        }
        RecoveryDecision::RestoreOld
            if inventario
                == (RecoveryInventory {
                    root: ArtifactState::Absent,
                    backup: ArtifactState::Old,
                    stage: ArtifactState::Absent,
                    root_new_validated: false,
                })
                && decision.steps(inventario)
                    == [
                        RecoveryStep::RemoveKnownStage,
                        RecoveryStep::RestoreBackupToRoot,
                        RecoveryStep::RemoveJournal,
                    ] =>
        {
            Some(FilaRecoverySoportada::RestoreOld)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, RawTreeDigest};

    fn identidad(id: u64) -> DirectoryIdentity {
        #[cfg(unix)]
        {
            DirectoryIdentity::unix(7, id)
        }
        #[cfg(windows)]
        {
            DirectoryIdentity::windows(7, format!("{id:032x}"))
        }
    }

    fn fingerprint(id: u64, byte: char) -> ArtifactFingerprint {
        ArtifactFingerprint::new(
            identidad(id),
            RawTreeDigest::from_hex(byte.to_string().repeat(64)).unwrap(),
        )
    }

    fn journal(state: JournalState) -> Journal {
        let mut journal = Journal::new(
            "0123456789abcdef0123456789abcdef",
            identidad(1),
            fingerprint(2, 'a'),
            fingerprint(3, 'b'),
        )
        .unwrap();
        for next in match state {
            JournalState::Prepared => &[][..],
            JournalState::BackupReady => &[JournalState::BackupReady][..],
            JournalState::Installed => &[JournalState::BackupReady, JournalState::Installed][..],
            JournalState::Committed => &[
                JournalState::BackupReady,
                JournalState::Installed,
                JournalState::Committed,
            ][..],
        } {
            journal.advance(*next).unwrap();
        }
        journal
    }

    fn inventario(
        root: ArtifactState,
        backup: ArtifactState,
        stage: ArtifactState,
    ) -> RecoveryInventory {
        RecoveryInventory {
            root,
            backup,
            stage,
            root_new_validated: false,
        }
    }

    #[test]
    fn journal_ausente_no_invoca_observacion() {
        let mut observado = false;
        let resultado = preflight_desde_journal_canonico(None, |_| {
            observado = true;
            unreachable!("no se observa inventario sin journal")
        })
        .unwrap();

        assert!(matches!(
            resultado,
            ResultadoPreflightRecovery::JournalAusente
        ));
        assert!(!observado);
    }

    #[test]
    fn fase_committed_no_invoca_observacion() {
        let mut observado = false;
        let committed = journal(JournalState::Committed);
        let resultado = observar_si_fase_soportada(&committed, |_| {
            observado = true;
            unreachable!("Committed se rechaza antes de observar inventario")
        })
        .unwrap();

        assert_eq!(resultado, None);
        assert!(!observado);
    }

    #[test]
    fn clasifica_solo_keep_old_y_restore_old_en_fases_permitidas() {
        let keep_old = inventario(
            ArtifactState::Old,
            ArtifactState::Absent,
            ArtifactState::Absent,
        );
        let restore_old = inventario(
            ArtifactState::Absent,
            ArtifactState::Old,
            ArtifactState::Absent,
        );

        for state in [
            JournalState::Prepared,
            JournalState::BackupReady,
            JournalState::Installed,
        ] {
            let journal = journal(state);
            assert_eq!(
                clasificar_fila_recovery(&journal, keep_old),
                Some(FilaRecoverySoportada::KeepOld),
                "KeepOld en {state:?}"
            );
            assert_eq!(
                clasificar_fila_recovery(&journal, restore_old),
                Some(FilaRecoverySoportada::RestoreOld),
                "RestoreOld en {state:?}"
            );
        }
        let committed = journal(JournalState::Committed);
        assert_eq!(clasificar_fila_recovery(&committed, keep_old), None);
        assert_eq!(clasificar_fila_recovery(&committed, restore_old), None);
    }

    #[test]
    fn toda_fila_distinta_a_keep_old_o_restore_old_es_unsupported() {
        let estados = [
            ArtifactState::Absent,
            ArtifactState::Old,
            ArtifactState::New,
            ArtifactState::Unknown,
        ];
        let fases = [
            JournalState::Prepared,
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ];
        for fase in fases {
            let journal = journal(fase);
            for root in estados {
                for backup in estados {
                    for stage in estados {
                        let inventario = inventario(root, backup, stage);
                        let exact_keep = fase != JournalState::Committed
                            && root == ArtifactState::Old
                            && backup == ArtifactState::Absent
                            && stage == ArtifactState::Absent;
                        let exact_restore = fase != JournalState::Committed
                            && root == ArtifactState::Absent
                            && backup == ArtifactState::Old
                            && stage == ArtifactState::Absent;
                        let esperado = if exact_keep {
                            Some(FilaRecoverySoportada::KeepOld)
                        } else if exact_restore {
                            Some(FilaRecoverySoportada::RestoreOld)
                        } else {
                            None
                        };
                        assert_eq!(
                            clasificar_fila_recovery(&journal, inventario),
                            esperado,
                            "{fase:?} root={root:?} backup={backup:?} stage={stage:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn stage_presente_o_desconocido_falla_cerrado() {
        let journal = journal(JournalState::Installed);
        for stage in [
            ArtifactState::Old,
            ArtifactState::New,
            ArtifactState::Unknown,
        ] {
            let keep_like = inventario(ArtifactState::Old, ArtifactState::Absent, stage);
            let restore_like = inventario(ArtifactState::Absent, ArtifactState::Old, stage);
            assert_eq!(clasificar_fila_recovery(&journal, keep_like), None);
            assert_eq!(clasificar_fila_recovery(&journal, restore_like), None);
        }
    }

    #[test]
    fn root_new_validated_falla_cerrado_para_ambas_filas() {
        let journal = journal(JournalState::Installed);
        for inventario in [
            inventario(
                ArtifactState::Old,
                ArtifactState::Absent,
                ArtifactState::Absent,
            ),
            inventario(
                ArtifactState::Absent,
                ArtifactState::Old,
                ArtifactState::Absent,
            ),
        ] {
            let inventario = RecoveryInventory {
                root_new_validated: true,
                ..inventario
            };
            assert_eq!(clasificar_fila_recovery(&journal, inventario), None);
        }
    }
}
