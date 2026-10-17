//! Selector puro del frame FELIX-RETIRE; elige un decoder y no hace fallback.

use super::super::intent_persistido_v2::{
    IntentRetiroPersistidoV2, MAX_RETIRE_V2_BYTES, RETIRE_V2_VERSION,
};
use super::{
    ErrorSelectorIntentRetiro, IntentRetiroPersistido, IntentRetiroPersistidoVersionado, MAGIC,
    MAX_INTENT_BYTES, VERSION,
};

const VERSION_BYTES: usize = 2;
const HEADER_END: usize = MAGIC.len() + VERSION_BYTES;
const MAX_VERSIONED_FRAME_BYTES: usize = maximo_de_cuotas(MAX_INTENT_BYTES, MAX_RETIRE_V2_BYTES);

pub(super) const fn cuota_maxima_frame_bytes() -> usize {
    MAX_VERSIONED_FRAME_BYTES
}

const fn maximo_de_cuotas(cuota_v1: usize, cuota_v2: usize) -> usize {
    if cuota_v1 > cuota_v2 {
        cuota_v1
    } else {
        cuota_v2
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VersionRetiro {
    V1,
    V2,
}

fn clasificar(frame: &[u8]) -> Result<VersionRetiro, ErrorSelectorIntentRetiro> {
    clasificar_con_limite(frame, MAX_VERSIONED_FRAME_BYTES)
}

fn clasificar_con_limite(
    frame: &[u8],
    limite: usize,
) -> Result<VersionRetiro, ErrorSelectorIntentRetiro> {
    if frame.len() > limite {
        return Err(ErrorSelectorIntentRetiro::TotalExcedeLimite);
    }
    if frame.len() < HEADER_END {
        return Err(ErrorSelectorIntentRetiro::HeaderTruncado);
    }
    if &frame[..MAGIC.len()] != MAGIC {
        return Err(ErrorSelectorIntentRetiro::MagicInvalido);
    }

    let version = u16::from_be_bytes(
        frame[MAGIC.len()..HEADER_END]
            .try_into()
            .expect("el encabezado comprobado contiene la versión"),
    );
    match version {
        VERSION => Ok(VersionRetiro::V1),
        RETIRE_V2_VERSION => Ok(VersionRetiro::V2),
        _ => Err(ErrorSelectorIntentRetiro::VersionNoSoportada),
    }
}

#[cfg(test)]
fn clasificar_con_cuotas(
    frame: &[u8],
    cuota_v1: usize,
    cuota_v2: usize,
) -> Result<VersionRetiro, ErrorSelectorIntentRetiro> {
    clasificar_con_limite(frame, maximo_de_cuotas(cuota_v1, cuota_v2))
}

pub(super) fn decodificar(
    frame: &[u8],
) -> Result<IntentRetiroPersistidoVersionado, ErrorSelectorIntentRetiro> {
    match clasificar(frame)? {
        VersionRetiro::V1 => IntentRetiroPersistido::decode(frame)
            .map(IntentRetiroPersistidoVersionado::V1)
            .map_err(|_| ErrorSelectorIntentRetiro::DecoderV1),
        VersionRetiro::V2 => IntentRetiroPersistidoV2::decode(frame)
            .map(IntentRetiroPersistidoVersionado::V2)
            .map_err(|_| ErrorSelectorIntentRetiro::DecoderV2),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
    use crate::transaccion::journal::v2::JournalV2;
    use crate::transaccion::journal::{
        ArtifactFingerprint, DirectoryIdentity, Journal, JournalState, RawTreeDigest,
    };

    use super::super::super::intent_persistido_v2::{
        IntentRetiroPersistidoV2, MAX_RETIRE_V2_BYTES,
    };
    use super::super::{
        ErrorSelectorIntentRetiro, IntentRetiroPersistido, IntentRetiroPersistidoVersionado, MAGIC,
        MAX_INTENT_BYTES, VERSION,
    };
    use super::{
        MAX_VERSIONED_FRAME_BYTES, VersionRetiro, clasificar, clasificar_con_cuotas, decodificar,
        maximo_de_cuotas,
    };

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
    const SCOPE: u64 = 0x0102_0304_0506_0708;
    const CONTEXT_LENGTH: u64 = 123;
    const CONTEXT_DIGEST: [u8; 32] = [0xD4; 32];

    fn file_identity(value: u64) -> IdentidadArchivoPersistida {
        #[cfg(unix)]
        let payload = format!(r#"{{"platform":"unix-v1","device":17,"inode":{value}}}"#);
        #[cfg(windows)]
        let payload =
            format!(r#"{{"platform":"windows-v1","volume":17,"file_id":"{value:032x}"}}"#);

        let mut frame = b"FELIX-FILEID\0".to_vec();
        frame.extend_from_slice(&1_u16.to_be_bytes());
        frame.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
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

    fn journal_v1() -> Journal {
        let mut journal = Journal::new(
            TRANSACTION_ID,
            directory_identity(29),
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
        for state in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            journal.advance(state).unwrap();
        }
        journal
    }

    fn journal_v2() -> JournalV2 {
        let mut journal = JournalV2::new(
            TRANSACTION_ID,
            directory_identity(29),
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
        for state in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            journal.advance(state).unwrap();
        }
        journal
    }

    fn record_v1() -> IntentRetiroPersistido {
        IntentRetiroPersistido::new(
            journal_v1(),
            file_identity(41),
            file_identity(42),
            file_identity(43),
            SCOPE,
            CONTEXT_LENGTH,
            CONTEXT_DIGEST,
        )
        .unwrap()
    }

    fn record_v2() -> IntentRetiroPersistidoV2 {
        IntentRetiroPersistidoV2::desde_datos_persistidos(
            journal_v2(),
            file_identity(41),
            file_identity(42),
            file_identity(43),
            SCOPE,
            CONTEXT_LENGTH,
            CONTEXT_DIGEST,
        )
        .unwrap()
    }

    fn replace_journal(frame: &[u8], journal: &[u8]) -> Vec<u8> {
        let length_offset = MAGIC.len() + 2;
        let mut offset = length_offset;
        let old_length = usize::from(read_u16(frame, &mut offset));
        let start = offset;
        let mut replaced = frame.to_vec();
        drop(replaced.splice(start..start + old_length, journal.iter().copied()));
        replaced[length_offset..length_offset + 2]
            .copy_from_slice(&u16::try_from(journal.len()).unwrap().to_be_bytes());
        replaced
    }

    fn read_u16(frame: &[u8], offset: &mut usize) -> u16 {
        let end = *offset + 2;
        let value = u16::from_be_bytes(frame[*offset..end].try_into().unwrap());
        *offset = end;
        value
    }

    fn header(version: u16) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&version.to_be_bytes());
        bytes
    }

    #[test]
    fn dispatch_conserva_el_dto_completo_de_cada_version_y_redacta_debug() {
        let v1 = record_v1();
        let v2 = record_v2();
        let frame_v1 = v1.encode().unwrap();
        let frame_v2 = v2.encode().unwrap();
        let decoded_v1 = decodificar(&frame_v1).unwrap();
        let decoded_v2 = decodificar(&frame_v2).unwrap();

        assert!(matches!(
            &decoded_v1,
            IntentRetiroPersistidoVersionado::V1(_)
        ));
        assert!(matches!(
            &decoded_v2,
            IntentRetiroPersistidoVersionado::V2(_)
        ));
        assert_eq!(decoded_v1, IntentRetiroPersistidoVersionado::V1(v1));
        assert_eq!(decoded_v2, IntentRetiroPersistidoVersionado::V2(v2));
        assert_eq!(
            format!("{decoded_v1:?}"),
            "IntentRetiroPersistidoVersionado::V1([redacted])"
        );
        assert_eq!(
            format!("{decoded_v2:?}"),
            "IntentRetiroPersistidoVersionado::V2([redacted])"
        );
        assert!(!format!("{decoded_v1:?}").contains(TRANSACTION_ID));
        assert!(!format!("{decoded_v2:?}").contains(TRANSACTION_ID));
        assert_eq!(
            MAX_VERSIONED_FRAME_BYTES,
            maximo_de_cuotas(MAX_INTENT_BYTES, MAX_RETIRE_V2_BYTES)
        );
    }

    #[test]
    fn journal_cruzado_falla_en_el_decoder_elegido_sin_fallback() {
        let journal_v1 = journal_v1().encode().unwrap();
        let journal_v2 = journal_v2().encode().unwrap();
        let frame_v1 = replace_journal(&record_v1().encode().unwrap(), &journal_v2);
        let frame_v2 = replace_journal(&record_v2().encode().unwrap(), &journal_v1);

        assert_eq!(clasificar(&frame_v1), Ok(VersionRetiro::V1));
        assert_eq!(clasificar(&frame_v2), Ok(VersionRetiro::V2));
        assert_eq!(
            decodificar(&frame_v1),
            Err(ErrorSelectorIntentRetiro::DecoderV1)
        );
        assert_eq!(
            decodificar(&frame_v2),
            Err(ErrorSelectorIntentRetiro::DecoderV2)
        );
    }

    #[test]
    fn headers_magic_y_version_se_rechazan_antes_de_delegar() {
        assert_eq!(
            clasificar(&[]),
            Err(ErrorSelectorIntentRetiro::HeaderTruncado)
        );

        let mut bad_magic = header(VERSION);
        bad_magic[0] ^= 1;
        assert_eq!(
            clasificar(&bad_magic),
            Err(ErrorSelectorIntentRetiro::MagicInvalido)
        );

        for version in [0, 3, u16::MAX] {
            assert_eq!(
                clasificar(&header(version)),
                Err(ErrorSelectorIntentRetiro::VersionNoSoportada)
            );
        }
    }

    #[test]
    fn limite_productivo_precede_la_lectura_del_header() {
        let oversized = vec![0; MAX_VERSIONED_FRAME_BYTES + 1];
        assert_eq!(
            clasificar(&oversized),
            Err(ErrorSelectorIntentRetiro::TotalExcedeLimite)
        );
        assert_eq!(
            clasificar_con_cuotas(&[0; 17], 8, 16),
            Err(ErrorSelectorIntentRetiro::TotalExcedeLimite)
        );
    }

    #[test]
    fn errores_selector_son_fijos_y_redactados() {
        let error_v1 = ErrorSelectorIntentRetiro::DecoderV1;
        let error_v2 = ErrorSelectorIntentRetiro::DecoderV2;
        for (error, expected) in [
            (error_v1, "retirement_selector_v1_invalid"),
            (error_v2, "retirement_selector_v2_invalid"),
        ] {
            assert_eq!(error.codigo(), expected);
            assert_eq!(error.to_string(), expected);
            assert_eq!(format!("{error:?}"), expected);
            assert!(!format!("{error:?}").contains(TRANSACTION_ID));
        }
    }

    #[cfg(test)]
    mod boundary_tests;
}
