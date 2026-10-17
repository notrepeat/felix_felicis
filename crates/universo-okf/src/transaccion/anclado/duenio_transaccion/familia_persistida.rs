use std::fmt;

use crate::transaccion::admision::envelope::selector::EnvelopeAdmisionVersionada;
use crate::transaccion::admision::envelope::v2::EnvelopeContextoAdmisionV2;
use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
use crate::transaccion::anclado::intent_persistido::IntentRetiroPersistidoVersionado;
use crate::transaccion::anclado::intent_persistido_v2::{
    ErrorIntentRetiroPersistidoV2, IntentRetiroPersistidoV2,
};
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
    RetiroA {
        journal: &'a JournalV2,
        contexto: &'a EnvelopeContextoAdmisionV2,
        intent: &'a IntentRetiroPersistidoV2,
    },
    RetiroBDatosParaReplay {
        contexto: &'a EnvelopeContextoAdmisionV2,
        intent: &'a IntentRetiroPersistidoV2,
    },
    RetiroCIntentSinContexto(&'a IntentRetiroPersistidoV2),
    VacioObservado,
}

impl fmt::Debug for FaseFamiliaPersistida<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fixed = match self {
            Self::LegadoV1(_) => "FaseFamiliaPersistida::LegadoV1([redacted])",
            Self::NormalV2 { .. } => "FaseFamiliaPersistida::NormalV2([redacted])",
            Self::RetiroA { .. } => "FaseFamiliaPersistida::RetiroA([redacted])",
            Self::RetiroBDatosParaReplay { .. } => {
                "FaseFamiliaPersistida::RetiroBDatosParaReplay([redacted])"
            }
            Self::RetiroCIntentSinContexto(_) => {
                "FaseFamiliaPersistida::RetiroCIntentSinContexto([redacted])"
            }
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
        (
            Some(JournalPersistido::V2(journal)),
            Some(EnvelopeAdmisionVersionada::V2(contexto)),
            Some(IntentRetiroPersistidoVersionado::V2(intent)),
        ) => {
            exigir_binding(contexto, journal)?;
            if journal != intent.journal() {
                return Err(ErrorFamiliaPersistida);
            }
            Ok(FaseFamiliaPersistida::RetiroA {
                journal,
                contexto,
                intent,
            })
        }
        (
            None,
            Some(EnvelopeAdmisionVersionada::V2(contexto)),
            Some(IntentRetiroPersistidoVersionado::V2(intent)),
        ) => {
            exigir_binding(contexto, intent.journal())?;
            Ok(FaseFamiliaPersistida::RetiroBDatosParaReplay { contexto, intent })
        }
        (None, None, Some(IntentRetiroPersistidoVersionado::V2(intent))) => {
            Ok(FaseFamiliaPersistida::RetiroCIntentSinContexto(intent))
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

/// Compara los bindings persistidos de las fases de retiro sin autorizar acciones.
pub(super) fn validar_bindings_retiro_datos(
    fase: &FaseFamiliaPersistida<'_>,
    intent: (&IdentidadArchivoPersistida, u64),
    journal: Option<(&IdentidadArchivoPersistida, u64, &JournalV2)>,
    contexto: Option<(&IdentidadArchivoPersistida, u64, u64, &[u8; 32])>,
) -> Result<(), ErrorIntentRetiroPersistidoV2> {
    let (intent_id, scope_intent) = intent;
    match fase {
        FaseFamiliaPersistida::RetiroA {
            journal: journal_de_fase,
            contexto: envelope,
            intent: registro,
        } => {
            let Some((journal_id, scope_journal, journal_observado)) = journal else {
                return Err(ErrorIntentRetiroPersistidoV2);
            };
            let Some((contexto_id, scope_contexto, longitud, digest)) = contexto else {
                return Err(ErrorIntentRetiroPersistidoV2);
            };

            if *journal_de_fase != journal_observado
                || scope_journal != scope_intent
                || scope_contexto != scope_intent
            {
                return Err(ErrorIntentRetiroPersistidoV2);
            }

            registro.validar_vinculo_archivo_intent_datos(intent_id, scope_intent)?;
            registro.validar_vinculo_journal_datos(journal_id, journal_observado)?;
            envelope
                .coincide_estructuralmente(journal_observado)
                .map_err(|_| ErrorIntentRetiroPersistidoV2)?;
            registro.validar_vinculo_contexto_datos(contexto_id, longitud, *digest)?;
            Ok(())
        }
        FaseFamiliaPersistida::RetiroBDatosParaReplay {
            contexto: envelope,
            intent: registro,
        } => {
            if journal.is_some() {
                return Err(ErrorIntentRetiroPersistidoV2);
            }
            let Some((contexto_id, scope_contexto, longitud, digest)) = contexto else {
                return Err(ErrorIntentRetiroPersistidoV2);
            };
            if scope_contexto != scope_intent {
                return Err(ErrorIntentRetiroPersistidoV2);
            }

            registro.validar_vinculo_archivo_intent_datos(intent_id, scope_intent)?;
            envelope
                .coincide_estructuralmente(registro.journal())
                .map_err(|_| ErrorIntentRetiroPersistidoV2)?;
            registro.validar_vinculo_contexto_datos(contexto_id, longitud, *digest)?;
            Ok(())
        }
        FaseFamiliaPersistida::RetiroCIntentSinContexto(registro) => {
            if journal.is_some() || contexto.is_some() {
                return Err(ErrorIntentRetiroPersistidoV2);
            }
            registro.validar_vinculo_archivo_intent_datos(intent_id, scope_intent)
        }
        _ => Err(ErrorIntentRetiroPersistidoV2),
    }
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
    use crate::transaccion::anclado::intent_persistido_v2::{
        ErrorIntentRetiroPersistidoV2, IntentRetiroPersistidoV2,
    };
    use crate::transaccion::journal::selector::JournalPersistido;
    use crate::transaccion::journal::v2::JournalV2;
    use crate::transaccion::journal::{
        ArtifactFingerprint, DirectoryIdentity, Journal, JournalState, RawTreeDigest,
    };

    use super::{FaseFamiliaPersistida, clasificar, validar_bindings_retiro_datos};

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

    fn envelope_v2_binding(journal: &JournalV2) -> EnvelopeContextoAdmisionV2 {
        EnvelopeContextoAdmisionV2::ligar(envelope_fixtures::contexto(b"\n"), journal)
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

    type IntentObservation<'a> = (&'a IdentidadArchivoPersistida, u64);
    type JournalObservation<'a> = (&'a IdentidadArchivoPersistida, u64, &'a JournalV2);
    type ContextObservation<'a> = (&'a IdentidadArchivoPersistida, u64, u64, &'a [u8; 32]);

    fn persisted_intent_v2(journal: JournalV2) -> IntentRetiroPersistidoV2 {
        match intent_v2(journal) {
            IntentRetiroPersistidoVersionado::V2(intent) => intent,
            IntentRetiroPersistidoVersionado::V1(_) => unreachable!("fixture is v2"),
        }
    }

    fn validate_a(
        phase_journal: &JournalV2,
        envelope: &EnvelopeContextoAdmisionV2,
        intent_record: &IntentRetiroPersistidoV2,
        intent_observed: IntentObservation<'_>,
        journal_observed: Option<JournalObservation<'_>>,
        context_observed: Option<ContextObservation<'_>>,
    ) -> Result<(), ErrorIntentRetiroPersistidoV2> {
        let phase = FaseFamiliaPersistida::RetiroA {
            journal: phase_journal,
            contexto: envelope,
            intent: intent_record,
        };
        validar_bindings_retiro_datos(&phase, intent_observed, journal_observed, context_observed)
    }

    fn assert_a_rejected(
        phase_journal: &JournalV2,
        envelope: &EnvelopeContextoAdmisionV2,
        intent_record: &IntentRetiroPersistidoV2,
        intent_observed: IntentObservation<'_>,
        journal_observed: Option<JournalObservation<'_>>,
        context_observed: Option<ContextObservation<'_>>,
    ) {
        assert!(
            validate_a(
                phase_journal,
                envelope,
                intent_record,
                intent_observed,
                journal_observed,
                context_observed,
            )
            .is_err()
        );
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
    fn tabla_completa_de_presencia_y_versiones_acepta_seis_familias() {
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
                        (0, 0, 0) | (1, 0, 0) | (2, 2, 0) | (2, 2, 2) | (0, 2, 2) | (0, 0, 2)
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
    fn normal_v2_exige_binding_y_estado_no_es_binding() {
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

    #[test]
    fn retiro_a_exige_journal_completo_incluido_estado() {
        let committed = journal_v2_committed(TRANSACTION_ID, 91);
        let persisted = JournalPersistido::V2(committed.clone());
        let context = envelope_v2(&committed);
        let matching_intent = intent_v2(committed.clone());
        assert!(matches!(
            clasificar(Some(&persisted), Some(&context), Some(&matching_intent)),
            Ok(FaseFamiliaPersistida::RetiroA { .. })
        ));

        let prepared = journal_v2(TRANSACTION_ID, 91);
        let prepared_persisted = JournalPersistido::V2(prepared.clone());
        let prepared_context = envelope_v2(&prepared);
        let error = clasificar(
            Some(&prepared_persisted),
            Some(&prepared_context),
            Some(&matching_intent),
        );
        assert!(error.is_err());
    }

    #[test]
    fn retiro_b_acepta_context_prepared_y_intent_committed_con_binding() {
        let prepared = journal_v2(TRANSACTION_ID, 101);
        let context = envelope_v2(&prepared);
        let committed = intent_v2(journal_v2_committed(TRANSACTION_ID, 101));
        assert!(matches!(
            clasificar(None, Some(&context), Some(&committed)),
            Ok(FaseFamiliaPersistida::RetiroBDatosParaReplay { .. })
        ));

        let other_parent = intent_v2(journal_v2_committed(TRANSACTION_ID, 102));
        assert_incompatible(None, Some(&context), Some(&other_parent));
    }

    #[test]
    fn retiro_c_solo_informa_intent_y_debug_se_mantiene_redactado() {
        let intent = intent_v2(journal_v2_committed(TRANSACTION_ID, 111));
        let phase = clasificar(None, None, Some(&intent)).unwrap();
        assert!(matches!(
            phase,
            FaseFamiliaPersistida::RetiroCIntentSinContexto(_)
        ));
        assert_eq!(
            format!("{phase:?}"),
            "FaseFamiliaPersistida::RetiroCIntentSinContexto([redacted])"
        );
        assert!(!format!("{phase:?}").contains(TRANSACTION_ID));
    }

    #[test]
    fn retiro_a_acepta_bindings_datos_coherentes() {
        let journal = journal_v2_committed(TRANSACTION_ID, 151);
        let envelope = envelope_v2_binding(&journal);
        let intent_record = persisted_intent_v2(journal.clone());
        let intent_id = identity_file_id(51);
        let journal_id = identity_file_id(52);
        let context_id = identity_file_id(53);
        let digest = [0x5a; 32];

        assert!(
            validate_a(
                &journal,
                &envelope,
                &intent_record,
                (&intent_id, 61),
                Some((&journal_id, 61, &journal)),
                Some((&context_id, 61, 1, &digest)),
            )
            .is_ok()
        );
    }

    #[test]
    fn retiro_a_rechaza_discrepancias_independientes() {
        let journal = journal_v2_committed(TRANSACTION_ID, 161);
        let envelope = envelope_v2_binding(&journal);
        let intent_record = persisted_intent_v2(journal.clone());
        let intent_id = identity_file_id(51);
        let journal_id = identity_file_id(52);
        let context_id = identity_file_id(53);
        let digest = [0x5a; 32];
        let journal_data = Some((&journal_id, 61, &journal));
        let context_data = Some((&context_id, 61, 1, &digest));

        assert_a_rejected(
            &journal,
            &envelope,
            &intent_record,
            (&identity_file_id(54), 61),
            journal_data,
            context_data,
        );
        assert_a_rejected(
            &journal,
            &envelope,
            &intent_record,
            (&intent_id, 62),
            journal_data,
            context_data,
        );
        assert_a_rejected(
            &journal,
            &envelope,
            &intent_record,
            (&intent_id, 62),
            Some((&journal_id, 62, &journal)),
            Some((&context_id, 62, 1, &digest)),
        );
        assert_a_rejected(
            &journal,
            &envelope,
            &intent_record,
            (&intent_id, 61),
            Some((&identity_file_id(55), 61, &journal)),
            context_data,
        );
        assert_a_rejected(
            &journal,
            &envelope,
            &intent_record,
            (&intent_id, 61),
            Some((&journal_id, 62, &journal)),
            context_data,
        );

        let other_journal = journal_v2_committed(OTHER_TRANSACTION_ID, 161);
        let other_intent = persisted_intent_v2(other_journal.clone());
        assert_a_rejected(
            &journal,
            &envelope,
            &other_intent,
            (&intent_id, 61),
            journal_data,
            context_data,
        );

        let phase_journal = journal_v2_committed(TRANSACTION_ID, 171);
        let observed_journal = journal_v2_committed(OTHER_TRANSACTION_ID, 171);
        let observed_envelope = envelope_v2_binding(&observed_journal);
        let observed_intent = persisted_intent_v2(observed_journal.clone());
        assert_a_rejected(
            &phase_journal,
            &observed_envelope,
            &observed_intent,
            (&intent_id, 61),
            Some((&journal_id, 61, &observed_journal)),
            context_data,
        );

        assert_a_rejected(
            &journal,
            &envelope,
            &intent_record,
            (&intent_id, 61),
            journal_data,
            Some((&identity_file_id(56), 61, 1, &digest)),
        );
        assert_a_rejected(
            &journal,
            &envelope,
            &intent_record,
            (&intent_id, 61),
            journal_data,
            Some((&context_id, 62, 1, &digest)),
        );
        assert_a_rejected(
            &journal,
            &envelope,
            &intent_record,
            (&intent_id, 61),
            journal_data,
            Some((&context_id, 61, 2, &digest)),
        );
        assert_a_rejected(
            &journal,
            &envelope,
            &intent_record,
            (&intent_id, 61),
            journal_data,
            Some((&context_id, 61, 1, &[0xa5; 32])),
        );

        let incompatible_envelope =
            envelope_v2_binding(&journal_v2_committed(OTHER_TRANSACTION_ID, 161));
        assert_a_rejected(
            &journal,
            &incompatible_envelope,
            &intent_record,
            (&intent_id, 61),
            journal_data,
            context_data,
        );
        assert_a_rejected(
            &journal,
            &envelope,
            &intent_record,
            (&intent_id, 61),
            None,
            context_data,
        );
        assert_a_rejected(
            &journal,
            &envelope,
            &intent_record,
            (&intent_id, 61),
            journal_data,
            None,
        );
    }

    #[test]
    fn bindings_rechazan_fases_no_retiro() {
        let journal_v1 = journal_v1_committed();
        let journal_v2 = journal_v2_committed(TRANSACTION_ID, 181);
        let envelope = envelope_v2_binding(&journal_v2);
        let intent_id = identity_file_id(51);
        let context_id = identity_file_id(53);
        let digest = [0x5a; 32];
        let intent = (&intent_id, 61);
        let context = Some((&context_id, 61, 1, &digest));
        let legacy_phase = FaseFamiliaPersistida::LegadoV1(&journal_v1);
        let normal_phase = FaseFamiliaPersistida::NormalV2 {
            journal: &journal_v2,
            contexto: &envelope,
        };
        let empty_phase = FaseFamiliaPersistida::VacioObservado;

        for phase in [&legacy_phase, &normal_phase, &empty_phase] {
            let error = validar_bindings_retiro_datos(phase, intent, None, context).unwrap_err();
            assert_eq!(format!("{error:?}"), "ErrorIntentRetiroPersistidoV2");
            assert!(!format!("{error:?}").contains(TRANSACTION_ID));
        }
    }

    #[test]
    fn retiro_b_liga_env2_al_journal_embebido_y_rechaza_formas_invalidas() {
        let intent_record = persisted_intent_v2(journal_v2_committed(TRANSACTION_ID, 191));
        let envelope = envelope_v2_binding(intent_record.journal());
        let intent_id = identity_file_id(51);
        let context_id = identity_file_id(53);
        let digest = [0x5a; 32];
        let phase = FaseFamiliaPersistida::RetiroBDatosParaReplay {
            contexto: &envelope,
            intent: &intent_record,
        };
        let context = Some((&context_id, 61, 1, &digest));

        assert!(validar_bindings_retiro_datos(&phase, (&intent_id, 61), None, context).is_ok());
        assert!(
            validar_bindings_retiro_datos(&phase, (&identity_file_id(54), 61), None, context,)
                .is_err()
        );
        assert!(
            validar_bindings_retiro_datos(
                &phase,
                (&intent_id, 61),
                None,
                Some((&context_id, 62, 1, &digest)),
            )
            .is_err()
        );
        assert!(
            validar_bindings_retiro_datos(
                &phase,
                (&intent_id, 62),
                None,
                Some((&context_id, 62, 1, &digest)),
            )
            .is_err()
        );
        assert!(
            validar_bindings_retiro_datos(
                &phase,
                (&intent_id, 61),
                Some((&identity_file_id(52), 61, intent_record.journal())),
                context,
            )
            .is_err()
        );
        assert!(validar_bindings_retiro_datos(&phase, (&intent_id, 61), None, None).is_err());
        assert!(
            validar_bindings_retiro_datos(
                &phase,
                (&intent_id, 61),
                None,
                Some((&identity_file_id(55), 61, 1, &digest)),
            )
            .is_err()
        );
        assert!(
            validar_bindings_retiro_datos(
                &phase,
                (&intent_id, 61),
                None,
                Some((&context_id, 61, 2, &digest)),
            )
            .is_err()
        );
        assert!(
            validar_bindings_retiro_datos(
                &phase,
                (&intent_id, 61),
                None,
                Some((&context_id, 61, 1, &[0xa5; 32])),
            )
            .is_err()
        );

        let incompatible = envelope_v2_binding(&journal_v2_committed(OTHER_TRANSACTION_ID, 191));
        let incompatible_phase = FaseFamiliaPersistida::RetiroBDatosParaReplay {
            contexto: &incompatible,
            intent: &intent_record,
        };
        assert!(
            validar_bindings_retiro_datos(&incompatible_phase, (&intent_id, 61), None, context)
                .is_err()
        );
    }

    #[test]
    fn retiro_c_compara_intent_sin_inferir_context_y_rechaza_datos_extra() {
        let intent_record = persisted_intent_v2(journal_v2_committed(TRANSACTION_ID, 201));
        let intent_id = identity_file_id(51);
        let journal_id = identity_file_id(52);
        let context_id = identity_file_id(53);
        let phase = FaseFamiliaPersistida::RetiroCIntentSinContexto(&intent_record);

        assert!(validar_bindings_retiro_datos(&phase, (&intent_id, 61), None, None).is_ok());
        assert!(validar_bindings_retiro_datos(&phase, (&intent_id, 62), None, None).is_err());
        assert!(
            validar_bindings_retiro_datos(&phase, (&identity_file_id(54), 61), None, None,)
                .is_err()
        );
        assert!(
            validar_bindings_retiro_datos(
                &phase,
                (&intent_id, 61),
                Some((&journal_id, 61, intent_record.journal())),
                None,
            )
            .is_err()
        );
        assert!(
            validar_bindings_retiro_datos(
                &phase,
                (&intent_id, 61),
                None,
                Some((&context_id, 61, 1, &[0x5a; 32])),
            )
            .is_err()
        );
    }
}
