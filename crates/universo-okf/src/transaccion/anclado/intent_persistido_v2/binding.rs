//! Comparaciones puras de los DTO persistidos; no conceden autoridad.

use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
use crate::transaccion::journal::v2::JournalV2;

use super::{ErrorIntentRetiroPersistidoV2, IntentRetiroPersistidoV2};

impl IntentRetiroPersistidoV2 {
    /// Compara el FILEID persistido y el scope observado como datos.
    pub(in crate::transaccion::anclado) fn validar_vinculo_archivo_intent_datos(
        &self,
        identidad: &IdentidadArchivoPersistida,
        scope: u64,
    ) -> Result<(), ErrorIntentRetiroPersistidoV2> {
        if identidad != &self.identidad_intent || scope != self.scope_parent {
            return Err(ErrorIntentRetiroPersistidoV2);
        }
        Ok(())
    }

    /// Compara FILEID y JournalV2 completo, con kind y estado retenidos.
    pub(in crate::transaccion::anclado) fn validar_vinculo_journal_datos(
        &self,
        identidad: &IdentidadArchivoPersistida,
        journal: &JournalV2,
    ) -> Result<(), ErrorIntentRetiroPersistidoV2> {
        if identidad != &self.identidad_journal || journal != &self.journal {
            return Err(ErrorIntentRetiroPersistidoV2);
        }
        Ok(())
    }

    /// Compara FILEID, longitud y digest recibidos; no lee el artefacto.
    pub(in crate::transaccion::anclado) fn validar_vinculo_contexto_datos(
        &self,
        identidad: &IdentidadArchivoPersistida,
        longitud: u64,
        digest: [u8; 32],
    ) -> Result<(), ErrorIntentRetiroPersistidoV2> {
        if identidad != &self.identidad_contexto
            || longitud != self.longitud_contexto
            || digest != self.digest_contexto
        {
            return Err(ErrorIntentRetiroPersistidoV2);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
    use crate::transaccion::journal::v2::JournalV2;
    use crate::transaccion::journal::{
        ArtifactFingerprint, DirectoryIdentity, JournalState, RawTreeDigest,
    };

    use super::super::{IntentRetiroPersistidoV2, MAX_CONTEXT_ARTIFACT_V2_BYTES};

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
    const CONTEXT_DIGEST: [u8; 32] = [0xD4; 32];

    fn identity_file_id(value: u64) -> IdentidadArchivoPersistida {
        #[cfg(unix)]
        let payload = format!(r#"{{"platform":"unix-v1","device":17,"inode":{value}}}"#);
        #[cfg(windows)]
        let payload =
            format!(r#"{{"platform":"windows-v1","volume":17,"file_id":"{value:032x}"}}"#);

        let mut frame = b"FELIX-FILEID\0".to_vec();
        frame.extend_from_slice(&1_u16.to_be_bytes());
        let payload_length = u32::try_from(payload.len()).unwrap();
        frame.extend_from_slice(&payload_length.to_be_bytes());
        frame.extend_from_slice(payload.as_bytes());
        IdentidadArchivoPersistida::decode(&mut Cursor::new(frame)).unwrap()
    }

    fn directory_identity(value: u64) -> DirectoryIdentity {
        #[cfg(unix)]
        {
            DirectoryIdentity::unix(17, value)
        }
        #[cfg(windows)]
        {
            DirectoryIdentity::windows(17, format!("{value:032x}"))
        }
    }

    fn journal(transaction_id: &str, parent: u64) -> JournalV2 {
        let mut record = JournalV2::new(
            transaction_id,
            directory_identity(parent),
            ArtifactFingerprint::new(
                directory_identity(31),
                RawTreeDigest::from_digest([0x11; 32]),
            ),
            ArtifactFingerprint::new(
                directory_identity(32),
                RawTreeDigest::from_digest([0x22; 32]),
            ),
        )
        .unwrap();
        for next in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            record.advance(next).unwrap();
        }
        record
    }

    fn record() -> IntentRetiroPersistidoV2 {
        IntentRetiroPersistidoV2::desde_datos_persistidos(
            journal(TRANSACTION_ID, 29),
            identity_file_id(41),
            identity_file_id(42),
            identity_file_id(43),
            53,
            MAX_CONTEXT_ARTIFACT_V2_BYTES,
            CONTEXT_DIGEST,
        )
        .unwrap()
    }

    #[test]
    fn intent_file_identity_and_scope_are_compared_independently() {
        let record = record();
        let expected = identity_file_id(41);

        assert_eq!(
            record.validar_vinculo_archivo_intent_datos(&expected, 53),
            Ok(())
        );
        assert!(
            record
                .validar_vinculo_archivo_intent_datos(&identity_file_id(44), 53)
                .is_err()
        );
        assert!(
            record
                .validar_vinculo_archivo_intent_datos(&expected, 54)
                .is_err()
        );
    }

    #[test]
    fn journal_binding_compares_file_identity_and_complete_record() {
        let record = record();
        let expected_journal = journal(TRANSACTION_ID, 29);

        assert_eq!(
            record.validar_vinculo_journal_datos(&identity_file_id(42), &expected_journal),
            Ok(())
        );
        assert!(
            record
                .validar_vinculo_journal_datos(&identity_file_id(45), &expected_journal)
                .is_err()
        );
        assert!(
            record
                .validar_vinculo_journal_datos(
                    &identity_file_id(42),
                    &journal("f123456789abcdef0123456789abcdef", 29),
                )
                .is_err()
        );
    }

    #[test]
    fn context_binding_compares_identity_length_and_digest_independently() {
        let record = record();
        let expected = identity_file_id(43);
        let context_length = MAX_CONTEXT_ARTIFACT_V2_BYTES;

        assert_eq!(
            record.validar_vinculo_contexto_datos(&expected, context_length, CONTEXT_DIGEST),
            Ok(())
        );
        assert!(
            record
                .validar_vinculo_contexto_datos(
                    &identity_file_id(46),
                    context_length,
                    CONTEXT_DIGEST,
                )
                .is_err()
        );
        assert!(
            record
                .validar_vinculo_contexto_datos(&expected, context_length - 1, CONTEXT_DIGEST)
                .is_err()
        );
        assert!(
            record
                .validar_vinculo_contexto_datos(&expected, context_length, [0xE5; 32])
                .is_err()
        );
    }
}
