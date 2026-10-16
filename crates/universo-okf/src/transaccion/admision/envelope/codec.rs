//! Encoder del envelope exterior de replay, puro y privado.
//!
//! El framing conserva el journal v1 canónico en `Prepared` y el contexto
//! `FELIX-ADMCTX` sin modificarlo. No concede autoridad ni realiza I/O.

use std::fmt;

use crate::transaccion::journal::{Journal, MAX_JOURNAL_BYTES};

use super::super::codec::{
    ErrorCodecAdmision, MAX_CODEC_BYTES, codificar_con_limite as codificar_contexto_con_limite,
};
use super::EnvelopeContextoAdmision;

const MAGIC: &[u8; 12] = b"FELIX-ADMENV";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = MAGIC.len() + 2 + 8;
const JOURNAL_LENGTH_BYTES: usize = 4;
const CONTEXT_LENGTH_BYTES: usize = 8;

#[allow(dead_code)]
mod decodificar;
mod lectura;
#[allow(dead_code)]
mod leer;
#[allow(dead_code, unused_imports)]
pub(crate) use decodificar::decodificar;
#[allow(dead_code, unused_imports)]
pub(crate) use leer::leer;

/// Codifica el vínculo y contexto en un framing único de cuota inclusiva.
pub(crate) fn codificar(
    envelope: &EnvelopeContextoAdmision,
) -> Result<Vec<u8>, ErrorCodecEnvelopeAdmision> {
    codificar_con_limite(envelope, MAX_CODEC_BYTES)
}

fn codificar_con_limite(
    envelope: &EnvelopeContextoAdmision,
    limite: usize,
) -> Result<Vec<u8>, ErrorCodecEnvelopeAdmision> {
    let limite = limite.min(MAX_CODEC_BYTES);
    let journal = Journal::new(
        envelope.vinculo.transaction_id.clone(),
        envelope.vinculo.parent.clone(),
        envelope.vinculo.old.clone(),
        envelope.vinculo.new.clone(),
    )
    .map_err(|_| ErrorCodecEnvelopeAdmision::JournalInvalido)?;
    let journal_bytes = journal
        .encode()
        .map_err(|_| ErrorCodecEnvelopeAdmision::JournalInvalido)?;
    if journal_bytes.len() > MAX_JOURNAL_BYTES {
        return Err(ErrorCodecEnvelopeAdmision::JournalInvalido);
    }

    let journal_start = HEADER_BYTES
        .checked_add(JOURNAL_LENGTH_BYTES)
        .ok_or(ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    let journal_end = journal_start
        .checked_add(journal_bytes.len())
        .ok_or(ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    let context_start = journal_end
        .checked_add(CONTEXT_LENGTH_BYTES)
        .ok_or(ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    let context_limit = limite
        .checked_sub(context_start)
        .ok_or(ErrorCodecEnvelopeAdmision::TotalExcedeLimite)?;

    // The inner codec preflights this remaining budget before reserving its
    // output, so the outer framing is part of the same inclusive limit.
    let mut output = codificar_contexto_con_limite(&envelope.contexto, context_limit)
        .map_err(mapear_error_contexto)?;
    let context_len = output.len();
    let total_len = context_start
        .checked_add(context_len)
        .ok_or(ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    if total_len > limite {
        return Err(ErrorCodecEnvelopeAdmision::TotalExcedeLimite);
    }

    let journal_len = u32::try_from(journal_bytes.len())
        .map_err(|_| ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    let total_bytes =
        u64::try_from(total_len).map_err(|_| ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    let context_bytes =
        u64::try_from(context_len).map_err(|_| ErrorCodecEnvelopeAdmision::LongitudExcedida)?;

    output
        .try_reserve_exact(context_start)
        .map_err(|_| ErrorCodecEnvelopeAdmision::ReservaFallida)?;
    output.resize(total_len, 0);
    output.copy_within(0..context_len, context_start);

    output[..MAGIC.len()].copy_from_slice(MAGIC);
    output[MAGIC.len()..MAGIC.len() + 2].copy_from_slice(&VERSION.to_be_bytes());
    output[MAGIC.len() + 2..HEADER_BYTES].copy_from_slice(&total_bytes.to_be_bytes());
    output[HEADER_BYTES..journal_start].copy_from_slice(&journal_len.to_be_bytes());
    output[journal_start..journal_end].copy_from_slice(&journal_bytes);
    output[journal_end..context_start].copy_from_slice(&context_bytes.to_be_bytes());

    Ok(output)
}

fn mapear_error_contexto(error: ErrorCodecAdmision) -> ErrorCodecEnvelopeAdmision {
    match error {
        ErrorCodecAdmision::TotalExcedeLimite => ErrorCodecEnvelopeAdmision::TotalExcedeLimite,
        ErrorCodecAdmision::LongitudExcedida => ErrorCodecEnvelopeAdmision::LongitudExcedida,
        ErrorCodecAdmision::ReservaFallida => ErrorCodecEnvelopeAdmision::ReservaFallida,
        ErrorCodecAdmision::ContextoInvalido
        | ErrorCodecAdmision::FramingInvalido
        | ErrorCodecAdmision::VersionNoSoportada
        | ErrorCodecAdmision::LimiteComponenteExcedido => {
            ErrorCodecEnvelopeAdmision::ContextoInvalido
        }
    }
}

/// Errores fijos y redactados del codec exterior.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorCodecEnvelopeAdmision {
    FramingInvalido,
    VersionNoSoportada,
    LecturaFallida,
    JournalInvalido,
    ContextoInvalido,
    TotalExcedeLimite,
    LongitudExcedida,
    ReservaFallida,
}

impl ErrorCodecEnvelopeAdmision {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::FramingInvalido => "admission_envelope_framing_invalid",
            Self::VersionNoSoportada => "admission_envelope_version_unsupported",
            Self::LecturaFallida => "admission_envelope_read_failed",
            Self::JournalInvalido => "admission_envelope_journal_invalid",
            Self::ContextoInvalido => "admission_envelope_context_invalid",
            Self::TotalExcedeLimite => "admission_envelope_limit",
            Self::LongitudExcedida => "admission_envelope_length_overflow",
            Self::ReservaFallida => "admission_envelope_allocation_failed",
        }
    }
}

impl fmt::Debug for ErrorCodecEnvelopeAdmision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ErrorCodecEnvelopeAdmision")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorCodecEnvelopeAdmision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorCodecEnvelopeAdmision {}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::transaccion::admision::ContextoAdmision;
    use crate::transaccion::admision::codec::codificar as codificar_contexto;
    use crate::transaccion::captura::{ConfigRawCapturada, materializar_snapshot_desde_entradas};
    use crate::transaccion::journal::{
        ArtifactFingerprint, DirectoryIdentity, JournalState, RawTreeDigest,
    };
    use crate::transaccion::snapshot::EntradaSnapshot;
    use universo_core::Delta;

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
    const MANIFIESTO: &str = r#"version = "envelope-codec-test/1"

[nodos]
sortes = ["a", "Retirado", "Patrón"]

[nodos.disco]
"Patrón" = "pattern"

[layout]
directorios = ["knowledge"]
sin_iota = []
ignorados = []
parent_key = "parent"
"#;
    const DOCUMENTO: &[u8] = b"---\ntype: a\ntitle: Index\nparent: \"\"\n---\nindex\n";

    fn contexto() -> ContextoAdmision {
        let configuracion =
            ConfigRawCapturada::nueva(MANIFIESTO.as_bytes(), b"\n".as_slice(), None, None).unwrap();
        let snapshot = materializar_snapshot_desde_entradas(
            vec![
                EntradaSnapshot::directorio("knowledge"),
                EntradaSnapshot::archivo("knowledge/index.md", DOCUMENTO),
            ],
            configuracion,
            "engine-envelope-codec/1",
            "policy-envelope-codec/1",
        )
        .unwrap();
        ContextoAdmision::capturar(&snapshot, &Delta::default()).unwrap()
    }

    fn identidad(seed: u64) -> DirectoryIdentity {
        #[cfg(windows)]
        {
            DirectoryIdentity::windows(seed, format!("{seed:032x}"))
        }
        #[cfg(unix)]
        {
            DirectoryIdentity::unix(seed, seed + 1)
        }
    }

    fn journal() -> Journal {
        let old = ArtifactFingerprint::new(
            identidad(2),
            RawTreeDigest::from_hex("a".repeat(64)).unwrap(),
        );
        let new = ArtifactFingerprint::new(
            identidad(3),
            RawTreeDigest::from_hex("b".repeat(64)).unwrap(),
        );
        Journal::new(TRANSACTION_ID, identidad(1), old, new).unwrap()
    }

    fn envelope(journal: &Journal) -> EnvelopeContextoAdmision {
        EnvelopeContextoAdmision::ligar(contexto(), journal)
    }

    #[test]
    fn codifica_determinista_y_siempre_incluye_journal_prepared() {
        let prepared = journal();
        let prepared_envelope = envelope(&prepared);
        let inner = codificar_contexto(&prepared_envelope.contexto).unwrap();
        let primero = codificar(&prepared_envelope).unwrap();
        let mut backup_ready = prepared.clone();
        backup_ready.advance(JournalState::BackupReady).unwrap();
        let mut installed = backup_ready.clone();
        installed.advance(JournalState::Installed).unwrap();
        let mut committed = installed.clone();
        committed.advance(JournalState::Committed).unwrap();

        for phased in [&backup_ready, &installed, &committed] {
            let state_before = phased.state();
            assert_eq!(codificar(&envelope(phased)).unwrap(), primero);
            assert_eq!(phased.state(), state_before);
        }
        assert_eq!(prepared.state(), JournalState::Prepared);
        assert_eq!(&primero[..MAGIC.len()], MAGIC);
        assert_eq!(
            u16::from_be_bytes(primero[12..14].try_into().unwrap()),
            VERSION
        );
        assert_eq!(
            u64::from_be_bytes(primero[14..22].try_into().unwrap()) as usize,
            primero.len()
        );
        let journal_len = u32::from_be_bytes(primero[22..26].try_into().unwrap()) as usize;
        let journal_end = 26 + journal_len;
        let embedded = Journal::parse(&primero[26..journal_end]).unwrap();
        assert_eq!(embedded.state(), JournalState::Prepared);
        let context_len =
            u64::from_be_bytes(primero[journal_end..journal_end + 8].try_into().unwrap()) as usize;
        assert_eq!(journal_end + 8 + context_len, primero.len());
        assert_eq!(&primero[journal_end + 8..], inner);
    }

    #[test]
    fn la_cuota_combinada_incluye_framing_y_es_inclusiva() {
        let envelope = envelope(&journal());
        let wire = codificar(&envelope).unwrap();

        assert_eq!(codificar_con_limite(&envelope, wire.len()).unwrap(), wire);
        assert_eq!(
            codificar_con_limite(&envelope, wire.len() - 1),
            Err(ErrorCodecEnvelopeAdmision::TotalExcedeLimite)
        );
    }

    #[test]
    fn los_errores_son_estables_y_no_revelan_el_contenido() {
        let envelope = envelope(&journal());
        let error = codificar_con_limite(&envelope, 1).unwrap_err();

        assert_eq!(error.codigo(), "admission_envelope_limit");
        assert_eq!(error.to_string(), error.codigo());
        assert_eq!(
            format!("{error:?}"),
            "ErrorCodecEnvelopeAdmision(\"admission_envelope_limit\")"
        );
        assert!(!format!("{error:?}").contains(TRANSACTION_ID));
        assert!(!format!("{error:?}").contains("raw"));
    }
}
