//! Codec puro para un intent privado de retiro.
//!
//! El DTO conserva comparadores de datos para una reapertura futura. No reconstruye
//! identidades físicas, no elimina archivos y no concede autoridad.

use std::fmt;

use crate::transaccion::admision::MAX_ENVELOPE_BYTES;
use crate::transaccion::anclado::IdentidadArchivo;
use crate::transaccion::anclado::ScopeFilesystem;
use crate::transaccion::anclado::identidad_archivo::MAX_PERSISTED_FILE_ID_FRAME_BYTES;
use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
use crate::transaccion::journal::{Journal, JournalState, MAX_JOURNAL_BYTES};

const MAGIC: &[u8; 13] = b"FELIX-RETIRE\0";
const VERSION: u16 = 1;
const VERSION_BYTES: usize = 2;
const JOURNAL_LENGTH_BYTES: usize = 2;
const IDENTITY_LENGTH_BYTES: usize = 2;
const SCOPE_BYTES: usize = 8;
const CONTEXT_LENGTH_BYTES: usize = 8;
const CONTEXT_DIGEST_BYTES: usize = 32;
const FIXED_BYTES: usize = MAGIC.len()
    + VERSION_BYTES
    + JOURNAL_LENGTH_BYTES
    + 3 * IDENTITY_LENGTH_BYTES
    + SCOPE_BYTES
    + CONTEXT_LENGTH_BYTES
    + CONTEXT_DIGEST_BYTES;
pub(super) const MAX_INTENT_BYTES: usize =
    FIXED_BYTES + MAX_JOURNAL_BYTES + 3 * MAX_PERSISTED_FILE_ID_FRAME_BYTES;

/// Registro persistible experimental; todos sus campos son comparadores de datos.
#[derive(PartialEq, Eq)]
pub(super) struct IntentRetiroPersistido {
    journal: Journal,
    identidad_intent: IdentidadArchivoPersistida,
    identidad_journal: IdentidadArchivoPersistida,
    identidad_contexto: IdentidadArchivoPersistida,
    scope_parent: u64,
    longitud_contexto: u64,
    digest_contexto: [u8; 32],
}

impl fmt::Debug for IntentRetiroPersistido {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IntentRetiroPersistido([redacted])")
    }
}

pub(super) struct ErrorIntentRetiroPersistido;

impl fmt::Debug for ErrorIntentRetiroPersistido {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ErrorIntentRetiroPersistido")
    }
}

impl fmt::Display for ErrorIntentRetiroPersistido {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid_persisted_retirement_intent")
    }
}

impl std::error::Error for ErrorIntentRetiroPersistido {}

impl IntentRetiroPersistido {
    /// Expone el Journal parseado como dato; no valida su vínculo con el parent.
    pub(super) fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Compara solo valores recibidos; no abre archivos ni consulta un owner.
    pub(super) fn validar_vinculo_archivo_intent(
        &self,
        identidad: IdentidadArchivo,
        scope: ScopeFilesystem,
    ) -> Result<(), ErrorIntentRetiroPersistido> {
        let identidad_observada = IdentidadArchivoPersistida::try_from(identidad)
            .map_err(|_| ErrorIntentRetiroPersistido)?;
        if identidad_observada != self.identidad_intent || scope.0 != self.scope_parent {
            return Err(ErrorIntentRetiroPersistido);
        }
        Ok(())
    }

    /// Compara identidad nativa y Journal completo por igualdad semántica; no hace I/O.
    /// Su consumidor de reconciliación se integrará en una unidad posterior.
    #[allow(dead_code)]
    pub(super) fn validar_vinculo_journal(
        &self,
        identidad: IdentidadArchivo,
        journal: &Journal,
    ) -> Result<(), ErrorIntentRetiroPersistido> {
        let identidad_observada = IdentidadArchivoPersistida::try_from(identidad)
            .map_err(|_| ErrorIntentRetiroPersistido)?;
        if identidad_observada != self.identidad_journal || journal != &self.journal {
            return Err(ErrorIntentRetiroPersistido);
        }
        Ok(())
    }

    /// Compara identidad nativa, longitud y digest recibidos; no hace I/O.
    /// Su consumidor de reconciliación se integrará en una unidad posterior.
    #[allow(dead_code)]
    pub(super) fn validar_vinculo_contexto(
        &self,
        identidad: IdentidadArchivo,
        longitud: u64,
        digest: [u8; 32],
    ) -> Result<(), ErrorIntentRetiroPersistido> {
        let identidad_observada = IdentidadArchivoPersistida::try_from(identidad)
            .map_err(|_| ErrorIntentRetiroPersistido)?;
        if identidad_observada != self.identidad_contexto
            || longitud != self.longitud_contexto
            || digest != self.digest_contexto
        {
            return Err(ErrorIntentRetiroPersistido);
        }
        Ok(())
    }

    fn new(
        journal: Journal,
        identidad_intent: IdentidadArchivoPersistida,
        identidad_journal: IdentidadArchivoPersistida,
        identidad_contexto: IdentidadArchivoPersistida,
        scope_parent: u64,
        longitud_contexto: u64,
        digest_contexto: [u8; 32],
    ) -> Result<Self, ErrorIntentRetiroPersistido> {
        let record = Self {
            journal,
            identidad_intent,
            identidad_journal,
            identidad_contexto,
            scope_parent,
            longitud_contexto,
            digest_contexto,
        };
        record.validar()?;
        Ok(record)
    }

    fn validar(&self) -> Result<(), ErrorIntentRetiroPersistido> {
        let max_contexto =
            u64::try_from(MAX_ENVELOPE_BYTES).map_err(|_| ErrorIntentRetiroPersistido)?;
        if self.journal.state() != JournalState::Committed
            || self.longitud_contexto == 0
            || self.longitud_contexto > max_contexto
        {
            return Err(ErrorIntentRetiroPersistido);
        }
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, ErrorIntentRetiroPersistido> {
        self.validar()?;
        let journal = self
            .journal
            .encode()
            .map_err(|_| ErrorIntentRetiroPersistido)?;
        if journal.is_empty() || journal.len() > MAX_JOURNAL_BYTES {
            return Err(ErrorIntentRetiroPersistido);
        }

        let identities = [
            encode_identity(&self.identidad_intent)?,
            encode_identity(&self.identidad_journal)?,
            encode_identity(&self.identidad_contexto)?,
        ];
        let total = identities
            .iter()
            .try_fold(
                FIXED_BYTES
                    .checked_add(journal.len())
                    .ok_or(ErrorIntentRetiroPersistido)?,
                |size, identity| size.checked_add(identity.len()),
            )
            .ok_or(ErrorIntentRetiroPersistido)?;
        if total > MAX_INTENT_BYTES {
            return Err(ErrorIntentRetiroPersistido);
        }

        let journal_length =
            u16::try_from(journal.len()).map_err(|_| ErrorIntentRetiroPersistido)?;
        let mut frame = Vec::with_capacity(total);
        frame.extend_from_slice(MAGIC);
        frame.extend_from_slice(&VERSION.to_be_bytes());
        frame.extend_from_slice(&journal_length.to_be_bytes());
        frame.extend_from_slice(&journal);
        for identity in identities {
            let length = u16::try_from(identity.len()).map_err(|_| ErrorIntentRetiroPersistido)?;
            frame.extend_from_slice(&length.to_be_bytes());
            frame.extend_from_slice(&identity);
        }
        frame.extend_from_slice(&self.scope_parent.to_be_bytes());
        frame.extend_from_slice(&self.longitud_contexto.to_be_bytes());
        frame.extend_from_slice(&self.digest_contexto);
        if frame.len() != total || frame.len() > MAX_INTENT_BYTES {
            return Err(ErrorIntentRetiroPersistido);
        }
        Ok(frame)
    }

    /// Decodifica framing en memoria; no prueba identidad de archivo ni admisión.
    pub(super) fn decode(frame: &[u8]) -> Result<Self, ErrorIntentRetiroPersistido> {
        if frame.len() > MAX_INTENT_BYTES {
            return Err(ErrorIntentRetiroPersistido);
        }
        let mut offset = 0;
        if take(frame, &mut offset, MAGIC.len())? != MAGIC {
            return Err(ErrorIntentRetiroPersistido);
        }
        if read_u16(frame, &mut offset)? != VERSION {
            return Err(ErrorIntentRetiroPersistido);
        }

        let journal_length = usize::from(read_u16(frame, &mut offset)?);
        if journal_length == 0 || journal_length > MAX_JOURNAL_BYTES {
            return Err(ErrorIntentRetiroPersistido);
        }
        let journal_bytes = take(frame, &mut offset, journal_length)?;
        let journal = Journal::parse(journal_bytes).map_err(|_| ErrorIntentRetiroPersistido)?;
        let identity_intent = read_identity(frame, &mut offset)?;
        let identity_journal = read_identity(frame, &mut offset)?;
        let identity_contexto = read_identity(frame, &mut offset)?;
        let scope_parent = read_u64(frame, &mut offset)?;
        let longitud_contexto = read_u64(frame, &mut offset)?;
        let digest_contexto = take(frame, &mut offset, CONTEXT_DIGEST_BYTES)?
            .try_into()
            .map_err(|_| ErrorIntentRetiroPersistido)?;
        if offset != frame.len() {
            return Err(ErrorIntentRetiroPersistido);
        }

        let record = Self::new(
            journal,
            identity_intent,
            identity_journal,
            identity_contexto,
            scope_parent,
            longitud_contexto,
            digest_contexto,
        )?;
        if record.encode()?.as_slice() != frame {
            return Err(ErrorIntentRetiroPersistido);
        }
        Ok(record)
    }
}

/// Codifica datos observados; nunca construye una identidad desde el frame.
pub(super) fn codificar_nuevo(
    journal: &Journal,
    identidad_intent: IdentidadArchivo,
    identidad_journal: IdentidadArchivo,
    identidad_contexto: IdentidadArchivo,
    scope_parent: u64,
    longitud_contexto: u64,
    digest_contexto: [u8; 32],
) -> Result<Vec<u8>, ErrorIntentRetiroPersistido> {
    let identidad_intent = IdentidadArchivoPersistida::try_from(identidad_intent)
        .map_err(|_| ErrorIntentRetiroPersistido)?;
    let identidad_journal = IdentidadArchivoPersistida::try_from(identidad_journal)
        .map_err(|_| ErrorIntentRetiroPersistido)?;
    let identidad_contexto = IdentidadArchivoPersistida::try_from(identidad_contexto)
        .map_err(|_| ErrorIntentRetiroPersistido)?;

    IntentRetiroPersistido::new(
        journal.clone(),
        identidad_intent,
        identidad_journal,
        identidad_contexto,
        scope_parent,
        longitud_contexto,
        digest_contexto,
    )?
    .encode()
}

fn encode_identity(
    identity: &IdentidadArchivoPersistida,
) -> Result<Vec<u8>, ErrorIntentRetiroPersistido> {
    let frame = identity.encode().map_err(|_| ErrorIntentRetiroPersistido)?;
    if frame.is_empty() || frame.len() > MAX_PERSISTED_FILE_ID_FRAME_BYTES {
        return Err(ErrorIntentRetiroPersistido);
    }
    Ok(frame)
}

fn read_identity(
    frame: &[u8],
    offset: &mut usize,
) -> Result<IdentidadArchivoPersistida, ErrorIntentRetiroPersistido> {
    let length = usize::from(read_u16(frame, offset)?);
    if length == 0 || length > MAX_PERSISTED_FILE_ID_FRAME_BYTES {
        return Err(ErrorIntentRetiroPersistido);
    }
    let bytes = take(frame, offset, length)?;
    let mut cursor = std::io::Cursor::new(bytes);
    IdentidadArchivoPersistida::decode(&mut cursor).map_err(|_| ErrorIntentRetiroPersistido)
}

fn read_u16(frame: &[u8], offset: &mut usize) -> Result<u16, ErrorIntentRetiroPersistido> {
    take(frame, offset, 2)?
        .try_into()
        .map(u16::from_be_bytes)
        .map_err(|_| ErrorIntentRetiroPersistido)
}

fn read_u64(frame: &[u8], offset: &mut usize) -> Result<u64, ErrorIntentRetiroPersistido> {
    take(frame, offset, 8)?
        .try_into()
        .map(u64::from_be_bytes)
        .map_err(|_| ErrorIntentRetiroPersistido)
}

fn take<'a>(
    frame: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8], ErrorIntentRetiroPersistido> {
    let end = offset
        .checked_add(length)
        .filter(|end| *end <= frame.len())
        .ok_or(ErrorIntentRetiroPersistido)?;
    let section = &frame[*offset..end];
    *offset = end;
    Ok(section)
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use crate::transaccion::anclado::ScopeFilesystem;
    use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
    use crate::transaccion::anclado::identidad_archivo::{IdentidadArchivo, identidad_archivo};
    use crate::transaccion::journal::{
        ArtifactFingerprint, DirectoryIdentity, Journal, JournalState, RawTreeDigest,
    };

    use super::{
        IntentRetiroPersistido, MAGIC, MAX_ENVELOPE_BYTES, MAX_INTENT_BYTES, MAX_JOURNAL_BYTES,
        VERSION,
    };

    mod extra_tests;
    mod journal_contexto_tests;

    fn directorio(id: u64) -> DirectoryIdentity {
        #[cfg(unix)]
        {
            DirectoryIdentity::unix(17, id)
        }
        #[cfg(windows)]
        {
            DirectoryIdentity::windows(17, format!("{id:032x}"))
        }
    }

    fn journal_committed() -> Journal {
        let old = ArtifactFingerprint::new(directorio(1), RawTreeDigest::from_digest([1; 32]));
        let new = ArtifactFingerprint::new(directorio(2), RawTreeDigest::from_digest([2; 32]));
        let mut journal =
            Journal::new("0123456789abcdef0123456789abcdef", directorio(3), old, new).unwrap();
        for next in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            journal.advance(next).unwrap();
        }
        journal
    }

    fn identity(path: &std::path::Path) -> IdentidadArchivoPersistida {
        let handle = File::open(path).unwrap();
        let native: IdentidadArchivo = identidad_archivo(&handle).unwrap();
        IdentidadArchivoPersistida::try_from(native).unwrap()
    }

    fn record(temp: &tempfile::TempDir, length: u64) -> IntentRetiroPersistido {
        let self_path = temp.path().join("intent");
        let journal_path = temp.path().join("journal");
        let context_path = temp.path().join("context");
        fs::write(&self_path, b"intent identity fixture").unwrap();
        fs::write(&journal_path, b"journal identity fixture").unwrap();
        fs::write(&context_path, b"context identity fixture").unwrap();
        IntentRetiroPersistido::new(
            journal_committed(),
            identity(&self_path),
            identity(&journal_path),
            identity(&context_path),
            19,
            length,
            [0x5a; 32],
        )
        .unwrap()
    }

    fn rejected(bytes: &[u8]) {
        assert!(IntentRetiroPersistido::decode(bytes).is_err());
    }

    #[test]
    fn roundtrip_committed_es_canonico_y_acotado() {
        let temp = tempfile::tempdir().unwrap();
        let intent = record(&temp, u64::try_from(MAX_ENVELOPE_BYTES).unwrap());
        let self_handle = File::open(temp.path().join("intent")).unwrap();
        let self_id = identidad_archivo(&self_handle).unwrap();
        let scope = ScopeFilesystem(19);
        let frame = intent.encode().unwrap();
        assert_eq!(MAX_INTENT_BYTES, 1536);
        assert!(frame.len() <= MAX_INTENT_BYTES);
        let decoded = IntentRetiroPersistido::decode(&frame).unwrap();
        assert_eq!(decoded, intent);
        assert!(decoded.journal() == intent.journal());
        decoded
            .validar_vinculo_archivo_intent(self_id, scope)
            .unwrap();
        assert_eq!(format!("{intent:?}"), "IntentRetiroPersistido([redacted])");
    }

    #[test]
    fn identidad_self_distinta_falla_mientras_ambos_archivos_siguen_abiertos() {
        let temp = tempfile::tempdir().unwrap();
        let intent = record(&temp, 1);
        let self_path = temp.path().join("intent");
        let decoy_path = temp.path().join("intent-decoy");
        fs::write(&decoy_path, b"intent identity fixture").unwrap();

        let self_handle = File::open(&self_path).unwrap();
        let decoy_handle = File::open(&decoy_path).unwrap();
        let self_id = identidad_archivo(&self_handle).unwrap();
        let decoy_id = identidad_archivo(&decoy_handle).unwrap();
        assert_ne!(self_id, decoy_id);
        assert!(
            intent
                .validar_vinculo_archivo_intent(self_id, ScopeFilesystem(19))
                .is_ok()
        );

        let error = intent
            .validar_vinculo_archivo_intent(decoy_id, ScopeFilesystem(19))
            .unwrap_err();
        assert_eq!(format!("{error:?}"), "ErrorIntentRetiroPersistido");
        assert_eq!(error.to_string(), "invalid_persisted_retirement_intent");
        assert_eq!(fs::read(&self_path).unwrap(), b"intent identity fixture");
        assert_eq!(fs::read(&decoy_path).unwrap(), b"intent identity fixture");
    }

    #[test]
    fn scope_distinto_con_id_correcto_falla_con_error_redactado() {
        let temp = tempfile::tempdir().unwrap();
        let intent = record(&temp, 1);
        let self_handle = File::open(temp.path().join("intent")).unwrap();
        let self_id = identidad_archivo(&self_handle).unwrap();
        let scope_sintetico = ScopeFilesystem(19 ^ 1);
        assert!(
            intent
                .validar_vinculo_archivo_intent(self_id, ScopeFilesystem(19))
                .is_ok()
        );

        let error = intent
            .validar_vinculo_archivo_intent(self_id, scope_sintetico)
            .unwrap_err();
        assert_eq!(format!("{error:?}"), "ErrorIntentRetiroPersistido");
        assert_eq!(error.to_string(), "invalid_persisted_retirement_intent");
    }

    #[test]
    fn rechaza_estado_no_committed_longitudes_y_bytes_extra() {
        let temp = tempfile::tempdir().unwrap();
        let intent = record(&temp, 1);
        let mut frame = intent.encode().unwrap();
        let journal_start = MAGIC.len() + 2 + 2;
        let journal_length = usize::from(u16::from_be_bytes(
            frame[MAGIC.len() + 2..journal_start].try_into().unwrap(),
        ));
        let journal_end = journal_start + journal_length;
        let journal = &mut frame[journal_start..journal_end];
        let state = journal
            .windows(b"committed".len())
            .position(|window| window == b"committed")
            .unwrap();
        journal[state..state + b"committed".len()].copy_from_slice(b"installed");
        rejected(&frame);

        let mut oversized_journal = intent.encode().unwrap();
        oversized_journal[MAGIC.len() + 2..journal_start]
            .copy_from_slice(&u16::try_from(MAX_JOURNAL_BYTES + 1).unwrap().to_be_bytes());
        rejected(&oversized_journal);

        let mut invalid_length = intent.encode().unwrap();
        let journal_length = usize::from(u16::from_be_bytes(
            invalid_length[MAGIC.len() + 2..journal_start]
                .try_into()
                .unwrap(),
        ));
        let first_identity_length_offset = journal_start + journal_length;
        invalid_length[first_identity_length_offset..first_identity_length_offset + 2]
            .copy_from_slice(&u16::MAX.to_be_bytes());
        rejected(&invalid_length);

        let mut zero_context_length = intent.encode().unwrap();
        let length_offset = zero_context_length.len() - 40;
        zero_context_length[length_offset..length_offset + 8].fill(0);
        rejected(&zero_context_length);

        let mut over_limit_context_length = intent.encode().unwrap();
        let length_offset = over_limit_context_length.len() - 40;
        over_limit_context_length[length_offset..length_offset + 8]
            .copy_from_slice(&(u64::try_from(MAX_ENVELOPE_BYTES).unwrap() + 1).to_be_bytes());
        rejected(&over_limit_context_length);

        let mut trailing = intent.encode().unwrap();
        trailing.push(0);
        rejected(&trailing);
        rejected(&trailing[..trailing.len() - 2]);
        rejected(&vec![0; MAX_INTENT_BYTES + 1]);
    }

    #[test]
    fn rechaza_version_y_redacta_error() {
        let temp = tempfile::tempdir().unwrap();
        let intent = record(&temp, 1);
        let mut frame = intent.encode().unwrap();
        frame[MAGIC.len()..MAGIC.len() + 2].copy_from_slice(&(VERSION + 1).to_be_bytes());
        let error = IntentRetiroPersistido::decode(&frame).unwrap_err();
        assert_eq!(format!("{error:?}"), "ErrorIntentRetiroPersistido");
        assert_eq!(error.to_string(), "invalid_persisted_retirement_intent");
        assert!(!format!("{intent:?}").contains("0123456789abcdef"));
    }

    #[test]
    fn wrapper_asigna_tres_ids_reales_a_sus_campos_en_orden() {
        let temp = tempfile::tempdir().unwrap();
        let self_path = temp.path().join("intent-self-canary");
        let journal_path = temp.path().join("journal-canary");
        let context_path = temp.path().join("context-canary");
        fs::write(&self_path, b"self file identity canary").unwrap();
        fs::write(&journal_path, b"journal file identity canary").unwrap();
        fs::write(&context_path, b"context file identity canary").unwrap();

        let self_handle = File::open(self_path).unwrap();
        let journal_handle = File::open(journal_path).unwrap();
        let context_handle = File::open(context_path).unwrap();
        let self_id = identidad_archivo(&self_handle).unwrap();
        let journal_id = identidad_archivo(&journal_handle).unwrap();
        let context_id = identidad_archivo(&context_handle).unwrap();
        assert_ne!(self_id, journal_id);
        assert_ne!(self_id, context_id);
        assert_ne!(journal_id, context_id);

        let journal = journal_committed();
        let scope_parent = 0x0102_0304_0506_0708;
        let context_length = 123;
        let context_digest = [0xa5; 32];
        let frame = super::codificar_nuevo(
            &journal,
            self_id,
            journal_id,
            context_id,
            scope_parent,
            context_length,
            context_digest,
        )
        .unwrap();

        let expected = IntentRetiroPersistido::new(
            journal,
            IdentidadArchivoPersistida::try_from(self_id).unwrap(),
            IdentidadArchivoPersistida::try_from(journal_id).unwrap(),
            IdentidadArchivoPersistida::try_from(context_id).unwrap(),
            scope_parent,
            context_length,
            context_digest,
        )
        .unwrap();
        let decoded = IntentRetiroPersistido::decode(&frame).unwrap();

        assert_eq!(decoded, expected);
    }
}
