use std::fmt;

use crate::transaccion::admision::envelope::selector::EnvelopeAdmisionVersionada;
use crate::transaccion::admision::envelope::v2::EnvelopeContextoAdmisionV2;
use crate::transaccion::anclado::intent_persistido::IntentRetiroPersistidoVersionado;
use crate::transaccion::journal::Journal;
use crate::transaccion::journal::selector::JournalPersistido;
use crate::transaccion::journal::v2::JournalV2;

/// Fase candidata basada únicamente en registros ya decodificados.
pub(super) enum FaseFamiliaPersistida<'a> {
    LegadoV1(&'a Journal),
    NormalV2 {
        journal: &'a JournalV2,
        contexto: &'a EnvelopeContextoAdmisionV2,
    },
    VacioObservado,
}

impl fmt::Debug for FaseFamiliaPersistida<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fixed = match self {
            Self::LegadoV1(_) => "FaseFamiliaPersistida::LegadoV1([redacted])",
            Self::NormalV2 { .. } => "FaseFamiliaPersistida::NormalV2([redacted])",
            Self::VacioObservado => "FaseFamiliaPersistida::VacioObservado",
        };
        formatter.write_str(fixed)
    }
}

pub(super) struct ErrorFamiliaPersistida;

impl ErrorFamiliaPersistida {
    pub(super) const fn codigo(&self) -> &'static str {
        "persisted_family_incompatible"
    }
}

impl fmt::Debug for ErrorFamiliaPersistida {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ErrorFamiliaPersistida([redacted])")
    }
}

impl fmt::Display for ErrorFamiliaPersistida {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorFamiliaPersistida {}

/// Clasifica la combinación exacta de familias sin consultar estado del filesystem.
pub(super) fn clasificar<'a>(
    journal: Option<&'a JournalPersistido>,
    contexto: Option<&'a EnvelopeAdmisionVersionada>,
    intent: Option<&'a IntentRetiroPersistidoVersionado>,
) -> Result<FaseFamiliaPersistida<'a>, ErrorFamiliaPersistida> {
    match (journal, contexto, intent) {
        (None, None, None) => Ok(FaseFamiliaPersistida::VacioObservado),
        (Some(JournalPersistido::V1(journal)), None, None) => {
            Ok(FaseFamiliaPersistida::LegadoV1(journal))
        }
        (
            Some(JournalPersistido::V2(journal)),
            Some(EnvelopeAdmisionVersionada::V2(contexto)),
            None,
        ) => {
            exigir_binding(contexto, journal)?;
            Ok(FaseFamiliaPersistida::NormalV2 { journal, contexto })
        }
        _ => Err(ErrorFamiliaPersistida),
    }
}

fn exigir_binding(
    contexto: &EnvelopeContextoAdmisionV2,
    journal: &JournalV2,
) -> Result<(), ErrorFamiliaPersistida> {
    contexto
        .coincide_estructuralmente(journal)
        .map_err(|_| ErrorFamiliaPersistida)
}

#[cfg(all(test, any(target_os = "linux", windows)))]
mod tests {
    use std::io::Cursor;

    use crate::transaccion::admision::envelope::tests as envelope_fixtures;
    use crate::transaccion::admision::envelope::v2::EnvelopeContextoAdmisionV2;
    use crate::transaccion::admision::envelope::{
        EnvelopeContextoAdmision, selector::EnvelopeAdmisionVersionada,
    };
    use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
    use crate::transaccion::anclado::intent_persistido::IntentRetiroPersistidoVersionado;
    use crate::transaccion::anclado::intent_persistido_v2::IntentRetiroPersistidoV2;
    use crate::transaccion::journal::selector::JournalPersistido;
    use crate::transaccion::journal::v2::JournalV2;
    use crate::transaccion::journal::{
        ArtifactFingerprint, DirectoryIdentity, Journal, JournalState, RawTreeDigest,
    };

    use super::{FaseFamiliaPersistida, clasificar};

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
    const OTHER_TRANSACTION_ID: &str = "fedcba9876543210fedcba9876543210";
    const INTENT_MAGIC_V1: &[u8] = b"FELIX-RETIRE\0";
    const FILE_ID_MAGIC: &[u8] = b"FELIX-FILEID\0";

    fn directory_identity(seed: u64) -> DirectoryIdentity {
        #[cfg(unix)]
        {
            DirectoryIdentity::unix(17, seed + 1)
        }
        #[cfg(windows)]
        {
            DirectoryIdentity::windows(17, format!("{seed:032x}"))
        }
    }

    fn fingerprint(seed: u64, digest: u8) -> ArtifactFingerprint {
        ArtifactFingerprint::new(
            directory_identity(seed),
            RawTreeDigest::from_digest([digest; 32]),
        )
    }

    fn journal_v1_committed() -> Journal {
        let context = envelope_fixtures::contexto(b"\n");
        let mut journal = envelope_fixtures::journal_valido(&context);
        for next in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            journal.advance(next).unwrap();
        }
        journal
    }

    fn journal_v2(transaction_id: &str, parent: u64) -> JournalV2 {
        JournalV2::new(
            transaction_id,
            directory_identity(parent),
            fingerprint(parent + 1, 0x11),
            fingerprint(parent + 2, 0x22),
        )
        .unwrap()
    }

    fn journal_v2_committed(transaction_id: &str, parent: u64) -> JournalV2 {
        let mut journal = journal_v2(transaction_id, parent);
        for next in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            journal.advance(next).unwrap();
        }
        journal
    }

    fn envelope_v1(journal: &Journal) -> EnvelopeAdmisionVersionada {
        EnvelopeAdmisionVersionada::V1(EnvelopeContextoAdmision::ligar(
            envelope_fixtures::contexto(b"\n"),
            journal,
        ))
    }

    fn envelope_v2(journal: &JournalV2) -> EnvelopeAdmisionVersionada {
        EnvelopeAdmisionVersionada::V2(EnvelopeContextoAdmisionV2::ligar(
            envelope_fixtures::contexto(b"\n"),
            journal,
        ))
    }

    fn identity_file_id_frame(seed: u64) -> Vec<u8> {
        #[cfg(unix)]
        let payload = format!(r#"{{"platform":"unix-v1","device":17,"inode":{seed}}}"#);
        #[cfg(windows)]
        let payload = format!(r#"{{"platform":"windows-v1","volume":17,"file_id":"{seed:032x}"}}"#);

        let mut frame = FILE_ID_MAGIC.to_vec();
        frame.extend_from_slice(&1_u16.to_be_bytes());
        frame.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
        frame.extend_from_slice(payload.as_bytes());
        frame
    }

    fn identity_file_id(seed: u64) -> IdentidadArchivoPersistida {
        IdentidadArchivoPersistida::decode(&mut Cursor::new(identity_file_id_frame(seed))).unwrap()
    }

    fn intent_v1(journal: &Journal) -> IntentRetiroPersistidoVersionado {
        let journal_bytes = journal.encode().unwrap();
        let mut frame = INTENT_MAGIC_V1.to_vec();
        frame.extend_from_slice(&1_u16.to_be_bytes());
        frame.extend_from_slice(&u16::try_from(journal_bytes.len()).unwrap().to_be_bytes());
        frame.extend_from_slice(&journal_bytes);
        for seed in 41..=43 {
            let identity = identity_file_id_frame(seed);
            frame.extend_from_slice(&u16::try_from(identity.len()).unwrap().to_be_bytes());
            frame.extend_from_slice(&identity);
        }
        frame.extend_from_slice(&53_u64.to_be_bytes());
        frame.extend_from_slice(&1_u64.to_be_bytes());
        frame.extend_from_slice(&[0xa5; 32]);
        crate::transaccion::anclado::intent_persistido::decodificar_versionado(&frame).unwrap()
    }

    fn intent_v2(journal: JournalV2) -> IntentRetiroPersistidoVersionado {
        IntentRetiroPersistidoVersionado::V2(
            IntentRetiroPersistidoV2::desde_datos_persistidos(
                journal,
                identity_file_id(51),
                identity_file_id(52),
                identity_file_id(53),
                61,
                1,
                [0x5a; 32],
            )
            .unwrap(),
        )
    }

    fn assert_incompatible(
        journal: Option<&JournalPersistido>,
        contexto: Option<&EnvelopeAdmisionVersionada>,
        intent: Option<&IntentRetiroPersistidoVersionado>,
    ) {
        let error = clasificar(journal, contexto, intent).unwrap_err();
        assert_eq!(error.codigo(), "persisted_family_incompatible");
    }

    #[test]
    fn tabla_completa_acepta_solo_vacio_legado_y_familia_normal_v2() {
        let journal_1 = JournalPersistido::V1(journal_v1_committed());
        let journal_2 = JournalPersistido::V2(journal_v2_committed(TRANSACTION_ID, 71));
        let envelope_1 = envelope_v1(match &journal_1 {
            JournalPersistido::V1(journal) => journal,
            JournalPersistido::V2(_) => unreachable!(),
        });
        let envelope_2 = envelope_v2(match &journal_2 {
            JournalPersistido::V1(_) => unreachable!(),
            JournalPersistido::V2(journal) => journal,
        });
        let intent_1 = intent_v1(match &journal_1 {
            JournalPersistido::V1(journal) => journal,
            JournalPersistido::V2(_) => unreachable!(),
        });
        let intent_2 = intent_v2(match &journal_2 {
            JournalPersistido::V1(_) => unreachable!(),
            JournalPersistido::V2(journal) => journal.clone(),
        });
        let journals = [None, Some(&journal_1), Some(&journal_2)];
        let envelopes = [None, Some(&envelope_1), Some(&envelope_2)];
        let intents = [None, Some(&intent_1), Some(&intent_2)];

        for (journal_index, journal) in journals.into_iter().enumerate() {
            for (envelope_index, envelope) in envelopes.into_iter().enumerate() {
                for (intent_index, intent) in intents.into_iter().enumerate() {
                    let accepted = matches!(
                        (journal_index, envelope_index, intent_index),
                        (0, 0, 0) | (1, 0, 0) | (2, 2, 0)
                    );
                    let result = clasificar(journal, envelope, intent);
                    assert_eq!(result.is_ok(), accepted);
                    if (journal_index, envelope_index, intent_index) == (1, 1, 1) {
                        assert!(result.is_err());
                    }
                }
            }
        }

        assert!(matches!(
            clasificar(None, None, None),
            Ok(FaseFamiliaPersistida::VacioObservado)
        ));
        assert!(matches!(
            clasificar(Some(&journal_1), None, None),
            Ok(FaseFamiliaPersistida::LegadoV1(_))
        ));
    }

    #[test]
    fn normal_v2_exige_binding_sin_tratar_estado_como_binding() {
        let prepared = journal_v2(TRANSACTION_ID, 81);
        let persisted = JournalPersistido::V2(prepared.clone());
        let matching_envelope = envelope_v2(&prepared);
        assert!(matches!(
            clasificar(Some(&persisted), Some(&matching_envelope), None),
            Ok(FaseFamiliaPersistida::NormalV2 { .. })
        ));

        let different_envelope = envelope_v2(&journal_v2(OTHER_TRANSACTION_ID, 81));
        assert_incompatible(Some(&persisted), Some(&different_envelope), None);

        let different_parent = JournalV2::new(
            TRANSACTION_ID,
            directory_identity(82),
            prepared.old().clone(),
            prepared.new_fingerprint().clone(),
        )
        .unwrap();
        let different_old = JournalV2::new(
            TRANSACTION_ID,
            prepared.parent().clone(),
            fingerprint(83, 0x33),
            prepared.new_fingerprint().clone(),
        )
        .unwrap();
        let different_new = JournalV2::new(
            TRANSACTION_ID,
            prepared.parent().clone(),
            prepared.old().clone(),
            fingerprint(84, 0x44),
        )
        .unwrap();
        for different_binding in [different_parent, different_old, different_new] {
            let envelope = envelope_v2(&different_binding);
            assert_incompatible(Some(&persisted), Some(&envelope), None);
        }

        let mut advanced = prepared.clone();
        advanced.advance(JournalState::BackupReady).unwrap();
        let advanced_persisted = JournalPersistido::V2(advanced.clone());
        let same_structural_binding = envelope_v2(&prepared);
        assert!(matches!(
            clasificar(
                Some(&advanced_persisted),
                Some(&same_structural_binding),
                None
            ),
            Ok(FaseFamiliaPersistida::NormalV2 { .. })
        ));
    }

    #[test]
    fn resultado_y_error_tienen_debug_fijo_y_redactado() {
        let journal = journal_v2(TRANSACTION_ID, 101);
        let persisted = JournalPersistido::V2(journal.clone());
        let envelope = envelope_v2(&journal);
        let phase = clasificar(Some(&persisted), Some(&envelope), None).unwrap();
        assert!(matches!(phase, FaseFamiliaPersistida::NormalV2 { .. }));
        assert_eq!(
            format!("{phase:?}"),
            "FaseFamiliaPersistida::NormalV2([redacted])"
        );
        assert!(!format!("{phase:?}").contains(TRANSACTION_ID));

        let incompatible = JournalPersistido::V2(journal_v2(TRANSACTION_ID, 111));
        let error = clasificar(Some(&incompatible), None, None).unwrap_err();
        assert_eq!(format!("{error:?}"), "ErrorFamiliaPersistida([redacted])");
        assert_eq!(error.to_string(), "persisted_family_incompatible");
        assert!(!format!("{error:?}").contains(TRANSACTION_ID));
    }
}
