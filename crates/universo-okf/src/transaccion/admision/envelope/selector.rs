//! Selector del frame exterior FELIX-ADMENV. Clasifica una vez, sin I/O ni autoridad.

use std::fmt;

use super::EnvelopeContextoAdmision;
use super::codec::{
    ErrorCodecEnvelopeAdmision, MAX_ENVELOPE_FRAME_BYTES, decodificar as decodificar_v1,
};
use super::v2::{
    EnvelopeContextoAdmisionV2, ErrorCodecEnvelopeContextoAdmisionV2,
    MAX_FRAME_BYTES as MAX_ADMENV_V2_FRAME_BYTES,
};
use crate::transaccion::journal::DirectoryIdentity;

const MAGIC: &[u8; 12] = b"FELIX-ADMENV";
const VERSION_BYTES: usize = 2;
const VERSION_END: usize = MAGIC.len() + VERSION_BYTES;
const MAX_VERSIONED_FRAME_BYTES: usize =
    maximo_de_cuotas(MAX_ENVELOPE_FRAME_BYTES, MAX_ADMENV_V2_FRAME_BYTES);

const fn maximo_de_cuotas(cuota_v1: usize, cuota_v2: usize) -> usize {
    if cuota_v1 > cuota_v2 {
        cuota_v1
    } else {
        cuota_v2
    }
}

/// Devuelve la única cuota familiar para preflight del framing exterior.
pub(crate) const fn cuota_maxima_frame_bytes() -> usize {
    MAX_VERSIONED_FRAME_BYTES
}

/// Variante exterior seleccionada por magic y versión; no proyecta datos ni autoriza acciones.
pub(crate) enum EnvelopeAdmisionVersionada {
    V1(EnvelopeContextoAdmision),
    V2(EnvelopeContextoAdmisionV2),
}

impl EnvelopeAdmisionVersionada {
    /// Delega la comparación de datos al envelope retenido sin proyectar su variante.
    pub(crate) fn coincide_parent(&self, parent: &DirectoryIdentity) -> bool {
        match self {
            Self::V1(envelope) => envelope.coincide_parent(parent),
            Self::V2(envelope) => envelope.coincide_parent(parent),
        }
    }
}

impl fmt::Debug for EnvelopeAdmisionVersionada {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V1(_) => formatter.write_str("EnvelopeAdmisionVersionada::V1([redacted])"),
            Self::V2(_) => formatter.write_str("EnvelopeAdmisionVersionada::V2([redacted])"),
        }
    }
}

/// Errores fijos del selector y los decoders elegidos; no conserva bytes ni DTO.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorSelectorEnvelopeAdmision {
    MagicInvalido,
    HeaderTruncado,
    VersionNoSoportada,
    TotalExcedeLimite,
    V1(ErrorCodecEnvelopeAdmision),
    V2(ErrorCodecEnvelopeContextoAdmisionV2),
}

impl ErrorSelectorEnvelopeAdmision {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::MagicInvalido => "admission_selector_magic_invalid",
            Self::HeaderTruncado => "admission_selector_header_truncated",
            Self::VersionNoSoportada => "admission_selector_version_unsupported",
            Self::TotalExcedeLimite => "admission_selector_total_exceeds_limit",
            Self::V1(error) => error.codigo(),
            Self::V2(error) => error.code(),
        }
    }
}

impl fmt::Debug for ErrorSelectorEnvelopeAdmision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorSelectorEnvelopeAdmision")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorSelectorEnvelopeAdmision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorSelectorEnvelopeAdmision {}

#[derive(Clone, Copy)]
enum VersionEnvelope {
    V1,
    V2,
}

/// Selecciona por framing exterior y delega directamente al decoder elegido.
pub(crate) fn decodificar(
    bytes: &[u8],
) -> Result<EnvelopeAdmisionVersionada, ErrorSelectorEnvelopeAdmision> {
    decodificar_con_limite(bytes, cuota_maxima_frame_bytes())
}

fn decodificar_con_limite(
    bytes: &[u8],
    limite: usize,
) -> Result<EnvelopeAdmisionVersionada, ErrorSelectorEnvelopeAdmision> {
    match clasificar(bytes, limite)? {
        VersionEnvelope::V1 => decodificar_v1(bytes)
            .map(EnvelopeAdmisionVersionada::V1)
            .map_err(ErrorSelectorEnvelopeAdmision::V1),
        VersionEnvelope::V2 => EnvelopeContextoAdmisionV2::decode(bytes)
            .map(EnvelopeAdmisionVersionada::V2)
            .map_err(ErrorSelectorEnvelopeAdmision::V2),
    }
}

fn clasificar(
    bytes: &[u8],
    limite: usize,
) -> Result<VersionEnvelope, ErrorSelectorEnvelopeAdmision> {
    if bytes.len() > limite.min(MAX_VERSIONED_FRAME_BYTES) {
        return Err(ErrorSelectorEnvelopeAdmision::TotalExcedeLimite);
    }
    if bytes.len() < VERSION_END {
        return Err(ErrorSelectorEnvelopeAdmision::HeaderTruncado);
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return Err(ErrorSelectorEnvelopeAdmision::MagicInvalido);
    }

    let version = u16::from_be_bytes(
        bytes[MAGIC.len()..VERSION_END]
            .try_into()
            .expect("el encabezado comprobado contiene la versión"),
    );
    match version {
        1 => Ok(VersionEnvelope::V1),
        2 => Ok(VersionEnvelope::V2),
        _ => Err(ErrorSelectorEnvelopeAdmision::VersionNoSoportada),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::admision::envelope::{EnvelopeContextoAdmision, tests as fixtures};
    use crate::transaccion::journal::v2::JournalV2;
    use crate::transaccion::journal::{ArtifactFingerprint, RawTreeDigest};

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
    const HEADER_BYTES: usize = MAGIC.len() + VERSION_BYTES + 8;
    const JOURNAL_LENGTH_BYTES: usize = 4;
    const JOURNAL_START: usize = HEADER_BYTES + JOURNAL_LENGTH_BYTES;

    fn digest(byte: char) -> RawTreeDigest {
        RawTreeDigest::from_hex(byte.to_string().repeat(64)).unwrap()
    }

    fn journal_v2() -> JournalV2 {
        JournalV2::new(
            TRANSACTION_ID,
            fixtures::identidad(1),
            ArtifactFingerprint::new(fixtures::identidad(2), digest('a')),
            ArtifactFingerprint::new(fixtures::identidad(3), digest('b')),
        )
        .unwrap()
    }

    fn frame_v1() -> (Vec<u8>, Vec<u8>) {
        let context = fixtures::contexto(b"\n");
        let journal = fixtures::journal_valido(&context);
        let journal_bytes = journal.encode().unwrap();
        let envelope = EnvelopeContextoAdmision::ligar(context, &journal);
        (
            super::super::codec::codificar(&envelope).unwrap(),
            journal_bytes,
        )
    }

    fn frame_v2() -> (Vec<u8>, Vec<u8>) {
        let journal = journal_v2();
        let journal_bytes = journal.encode().unwrap();
        let context = fixtures::contexto(b"\n");
        let envelope = EnvelopeContextoAdmisionV2::ligar(context, &journal);
        (envelope.encode().unwrap(), journal_bytes)
    }

    fn replace_journal(frame: &mut Vec<u8>, journal: &[u8]) {
        let old_len =
            u32::from_be_bytes(frame[HEADER_BYTES..JOURNAL_START].try_into().unwrap()) as usize;
        drop(frame.splice(
            JOURNAL_START..JOURNAL_START + old_len,
            journal.iter().copied(),
        ));
        let journal_len = u32::try_from(journal.len()).unwrap();
        frame[HEADER_BYTES..JOURNAL_START].copy_from_slice(&journal_len.to_be_bytes());
        let total_len = u64::try_from(frame.len()).unwrap();
        frame[MAGIC.len() + VERSION_BYTES..HEADER_BYTES].copy_from_slice(&total_len.to_be_bytes());
    }

    fn expect_error(bytes: &[u8]) -> ErrorSelectorEnvelopeAdmision {
        match decodificar(bytes) {
            Ok(_) => panic!("se esperaba rechazo del envelope"),
            Err(error) => error,
        }
    }

    #[test]
    fn selecciona_y_conserva_las_dos_variantes() {
        let (wire_v1, _) = frame_v1();
        let (wire_v2, _) = frame_v2();

        assert!(matches!(
            decodificar(&wire_v1).unwrap(),
            EnvelopeAdmisionVersionada::V1(_)
        ));
        assert!(matches!(
            decodificar(&wire_v2).unwrap(),
            EnvelopeAdmisionVersionada::V2(_)
        ));
    }

    #[test]
    fn rechaza_magic_version_y_header_truncado_antes_de_delegar() {
        assert!(matches!(
            expect_error(&[]),
            ErrorSelectorEnvelopeAdmision::HeaderTruncado
        ));

        let mut unknown_magic = vec![0; VERSION_END];
        unknown_magic[..MAGIC.len()].copy_from_slice(MAGIC);
        unknown_magic[0] = b'X';
        unknown_magic[MAGIC.len()..VERSION_END].copy_from_slice(&1_u16.to_be_bytes());
        assert!(matches!(
            expect_error(&unknown_magic),
            ErrorSelectorEnvelopeAdmision::MagicInvalido
        ));

        let (wire, _) = frame_v1();
        for version in [0, 3, u16::MAX] {
            let mut candidate = wire.clone();
            candidate[MAGIC.len()..VERSION_END].copy_from_slice(&version.to_be_bytes());
            assert!(matches!(
                expect_error(&candidate),
                ErrorSelectorEnvelopeAdmision::VersionNoSoportada
            ));
        }

        for (version, es_v1) in [(1_u16, true), (2_u16, false)] {
            let mut partial_header = vec![0; VERSION_END];
            partial_header[..MAGIC.len()].copy_from_slice(MAGIC);
            partial_header[MAGIC.len()..VERSION_END].copy_from_slice(&version.to_be_bytes());
            let error = expect_error(&partial_header);
            if es_v1 {
                assert!(matches!(error, ErrorSelectorEnvelopeAdmision::V1(_)));
            } else {
                assert!(matches!(error, ErrorSelectorEnvelopeAdmision::V2(_)));
            }
        }

        for mut candidate in [frame_v1().0, frame_v2().0] {
            candidate.truncate(VERSION_END - 1);
            assert!(matches!(
                expect_error(&candidate),
                ErrorSelectorEnvelopeAdmision::HeaderTruncado
            ));
        }
    }

    #[test]
    fn payload_de_journal_de_la_otra_version_conserva_el_decoder_elegido() {
        let (mut outer_v1, journal_v1) = frame_v1();
        let (_, journal_v2) = frame_v2();
        replace_journal(&mut outer_v1, &journal_v2);
        let error_v1 = expect_error(&outer_v1);
        assert!(matches!(error_v1, ErrorSelectorEnvelopeAdmision::V1(_)));
        assert_eq!(error_v1.codigo(), "admission_envelope_journal_invalid");

        let (mut outer_v2, _) = frame_v2();
        replace_journal(&mut outer_v2, &journal_v1);
        let error = expect_error(&outer_v2);
        assert!(matches!(error, ErrorSelectorEnvelopeAdmision::V2(_)));
        assert_eq!(error.codigo(), "admission_envelope_v2_journal_invalid");
        assert_eq!(error.to_string(), error.codigo());
        assert!(!format!("{error:?}").contains(TRANSACTION_ID));
    }

    #[test]
    fn truncamiento_y_trailing_llegan_solo_al_parser_seleccionado() {
        let (wire_v1, _) = frame_v1();
        let (wire_v2, _) = frame_v2();

        for (wire, es_v1) in [(&wire_v1, true), (&wire_v2, false)] {
            let truncado = &wire[..wire.len() - 1];
            let mut trailing = wire.clone();
            trailing.push(b'x');

            if es_v1 {
                assert!(matches!(
                    expect_error(truncado),
                    ErrorSelectorEnvelopeAdmision::V1(_)
                ));
                assert!(matches!(
                    expect_error(&trailing),
                    ErrorSelectorEnvelopeAdmision::V1(_)
                ));
            } else {
                assert!(matches!(
                    expect_error(truncado),
                    ErrorSelectorEnvelopeAdmision::V2(_)
                ));
                assert!(matches!(
                    expect_error(&trailing),
                    ErrorSelectorEnvelopeAdmision::V2(_)
                ));
            }
        }
    }

    #[test]
    fn cuota_seam_es_inclusivo_y_precede_la_clasificacion() {
        let (wire_v1, _) = frame_v1();
        let (wire_v2, _) = frame_v2();

        assert!(decodificar_con_limite(&wire_v1, wire_v1.len()).is_ok());
        assert!(matches!(
            decodificar_con_limite(&wire_v1, wire_v1.len() - 1),
            Err(ErrorSelectorEnvelopeAdmision::TotalExcedeLimite)
        ));
        assert!(decodificar_con_limite(&wire_v2, wire_v2.len()).is_ok());
        assert!(matches!(
            decodificar_con_limite(&wire_v2, wire_v2.len() - 1),
            Err(ErrorSelectorEnvelopeAdmision::TotalExcedeLimite)
        ));

        let oversized_invalid = vec![0; wire_v1.len() + 1];
        assert!(matches!(
            decodificar_con_limite(&oversized_invalid, wire_v1.len()),
            Err(ErrorSelectorEnvelopeAdmision::TotalExcedeLimite)
        ));
    }

    #[test]
    fn cap_comun_usa_la_mayor_cuota_sin_atar_v1_a_v2() {
        assert_eq!(maximo_de_cuotas(8, 16), 16);
        assert_eq!(maximo_de_cuotas(16, 8), 16);
        assert_eq!(maximo_de_cuotas(8, 8), 8);
        assert_eq!(
            cuota_maxima_frame_bytes(),
            maximo_de_cuotas(MAX_ENVELOPE_FRAME_BYTES, MAX_ADMENV_V2_FRAME_BYTES)
        );
    }

    #[test]
    fn parent_binding_delega_en_ambas_variantes_y_v2_conserva_kind() {
        let (wire_v1, _) = frame_v1();
        let parent = fixtures::identidad(1);
        let otro_parent = fixtures::identidad(9);
        let v1 = decodificar(&wire_v1).unwrap();

        assert!(v1.coincide_parent(&parent));
        assert!(!v1.coincide_parent(&otro_parent));

        let journal = journal_v2();
        let frame_v2 = EnvelopeContextoAdmisionV2::ligar(fixtures::contexto(b"\n"), &journal)
            .encode()
            .unwrap();
        let v2 = decodificar(&frame_v2).unwrap();

        assert!(v2.coincide_parent(&parent));
        assert!(!v2.coincide_parent(&otro_parent));
        match v2 {
            EnvelopeAdmisionVersionada::V2(envelope) => {
                assert!(envelope.coincide_estructuralmente(&journal).is_ok());
            }
            EnvelopeAdmisionVersionada::V1(_) => {
                panic!("el binding v2 debe conservar su variante y su kind cerrado");
            }
        }
    }

    #[test]
    fn error_selector_es_fijo_redactado_y_mantiene_codigos_de_decoder() {
        let (mut wire_v1, _) = frame_v1();
        let (mut wire_v2, _) = frame_v2();
        replace_journal(&mut wire_v1, b"private-v1-record");
        replace_journal(&mut wire_v2, b"private-v2-record");

        for error in [expect_error(&wire_v1), expect_error(&wire_v2)] {
            assert_eq!(error.to_string(), error.codigo());
            assert!(!format!("{error:?}").contains("private-"));
            assert!(!format!("{error:?}").contains(TRANSACTION_ID));
        }
    }
}
