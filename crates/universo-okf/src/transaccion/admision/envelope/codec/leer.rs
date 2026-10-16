//! Lector acotado de frames completos; no resuelve rutas ni consulta el filesystem.

use std::io::{self, Read};

use crate::transaccion::journal::MAX_JOURNAL_BYTES;

use super::super::EnvelopeContextoAdmision;
use super::{
    CONTEXT_LENGTH_BYTES, ErrorCodecEnvelopeAdmision, HEADER_BYTES, JOURNAL_LENGTH_BYTES, MAGIC,
    MAX_CODEC_BYTES, VERSION,
};

const HEADER_WITH_JOURNAL_LENGTH: usize = HEADER_BYTES + JOURNAL_LENGTH_BYTES;
const MIN_FRAME_BYTES: usize = HEADER_WITH_JOURNAL_LENGTH + CONTEXT_LENGTH_BYTES;

/// Lee un frame completo de un reader que debe alcanzar EOF tras sus bytes.
/// No admite streams persistentes ni decide cómo se obtiene el reader.
pub(crate) fn leer<R: Read>(
    reader: &mut R,
) -> Result<EnvelopeContextoAdmision, ErrorCodecEnvelopeAdmision> {
    leer_con_limite(reader, MAX_CODEC_BYTES)
}

fn leer_con_limite<R: Read>(
    reader: &mut R,
    limite: usize,
) -> Result<EnvelopeContextoAdmision, ErrorCodecEnvelopeAdmision> {
    let limite = limite.min(MAX_CODEC_BYTES);
    let mut header = [0_u8; HEADER_WITH_JOURNAL_LENGTH];
    leer_exact(reader, &mut header)?;

    if &header[..MAGIC.len()] != MAGIC {
        return Err(ErrorCodecEnvelopeAdmision::FramingInvalido);
    }
    let version = u16::from_be_bytes(
        header[MAGIC.len()..MAGIC.len() + 2]
            .try_into()
            .map_err(|_| ErrorCodecEnvelopeAdmision::FramingInvalido)?,
    );
    if version != VERSION {
        return Err(ErrorCodecEnvelopeAdmision::VersionNoSoportada);
    }

    let total = usize::try_from(u64::from_be_bytes(
        header[MAGIC.len() + 2..HEADER_BYTES]
            .try_into()
            .map_err(|_| ErrorCodecEnvelopeAdmision::FramingInvalido)?,
    ))
    .map_err(|_| ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    if total > limite {
        return Err(ErrorCodecEnvelopeAdmision::TotalExcedeLimite);
    }
    if total < MIN_FRAME_BYTES {
        return Err(ErrorCodecEnvelopeAdmision::FramingInvalido);
    }

    let journal_len = usize::try_from(u32::from_be_bytes(
        header[HEADER_BYTES..HEADER_WITH_JOURNAL_LENGTH]
            .try_into()
            .map_err(|_| ErrorCodecEnvelopeAdmision::FramingInvalido)?,
    ))
    .map_err(|_| ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    if journal_len > MAX_JOURNAL_BYTES {
        return Err(ErrorCodecEnvelopeAdmision::JournalInvalido);
    }

    let context_length_offset = HEADER_WITH_JOURNAL_LENGTH
        .checked_add(journal_len)
        .ok_or(ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    let context_offset = context_length_offset
        .checked_add(CONTEXT_LENGTH_BYTES)
        .ok_or(ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    if context_offset > total {
        return Err(ErrorCodecEnvelopeAdmision::FramingInvalido);
    }

    let mut journal = [0_u8; MAX_JOURNAL_BYTES];
    leer_exact(reader, &mut journal[..journal_len])?;
    let mut context_len_wire = [0_u8; CONTEXT_LENGTH_BYTES];
    leer_exact(reader, &mut context_len_wire)?;
    let context_len = usize::try_from(u64::from_be_bytes(context_len_wire))
        .map_err(|_| ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    let expected_total = context_offset
        .checked_add(context_len)
        .ok_or(ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    if expected_total > limite {
        return Err(ErrorCodecEnvelopeAdmision::TotalExcedeLimite);
    }
    if expected_total != total {
        return Err(ErrorCodecEnvelopeAdmision::FramingInvalido);
    }

    let mut encoded = Vec::new();
    encoded
        .try_reserve_exact(total)
        .map_err(|_| ErrorCodecEnvelopeAdmision::ReservaFallida)?;
    encoded.extend_from_slice(&header);
    encoded.extend_from_slice(&journal[..journal_len]);
    encoded.extend_from_slice(&context_len_wire);
    let context_start = encoded.len();
    encoded.resize(total, 0);
    leer_exact(reader, &mut encoded[context_start..])?;
    exigir_eof(reader)?;

    super::decodificar::decodificar(&encoded)
}

fn leer_exact<R: Read>(
    reader: &mut R,
    output: &mut [u8],
) -> Result<(), ErrorCodecEnvelopeAdmision> {
    reader.read_exact(output).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            ErrorCodecEnvelopeAdmision::FramingInvalido
        } else {
            ErrorCodecEnvelopeAdmision::LecturaFallida
        }
    })
}

fn exigir_eof<R: Read>(reader: &mut R) -> Result<(), ErrorCodecEnvelopeAdmision> {
    let mut sentinel = [0_u8; 1];
    loop {
        match reader.read(&mut sentinel) {
            Ok(0) => return Ok(()),
            Ok(_) => return Err(ErrorCodecEnvelopeAdmision::FramingInvalido),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorCodecEnvelopeAdmision::LecturaFallida),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::transaccion::admision::envelope::{EnvelopeContextoAdmision, tests as fixtures};
    use crate::transaccion::journal::Journal;

    use super::super::{HEADER_BYTES, JOURNAL_LENGTH_BYTES};

    struct ReaderControlado {
        bytes: Vec<u8>,
        posicion: usize,
        max_fragmento: usize,
        interrupciones: usize,
        fallar_en: Option<usize>,
        panic_despues: Option<usize>,
    }

    impl ReaderControlado {
        fn nuevo(bytes: Vec<u8>) -> Self {
            Self {
                bytes,
                posicion: 0,
                max_fragmento: usize::MAX,
                interrupciones: 0,
                fallar_en: None,
                panic_despues: None,
            }
        }
    }

    impl Read for ReaderControlado {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self
                .panic_despues
                .is_some_and(|limite| self.posicion >= limite)
            {
                panic!("lectura posterior al header rechazado");
            }
            if self.interrupciones > 0 {
                self.interrupciones -= 1;
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            if self.fallar_en.is_some_and(|limite| self.posicion >= limite) {
                return Err(io::Error::other("secret-path-marker"));
            }
            if self.posicion == self.bytes.len() {
                return Ok(0);
            }

            let disponible = self.bytes.len() - self.posicion;
            let hasta_fallo = self
                .fallar_en
                .map_or(usize::MAX, |limite| limite.saturating_sub(self.posicion));
            let cantidad = buffer
                .len()
                .min(self.max_fragmento)
                .min(disponible)
                .min(hasta_fallo);
            buffer[..cantidad]
                .copy_from_slice(&self.bytes[self.posicion..self.posicion + cantidad]);
            self.posicion += cantidad;
            Ok(cantidad)
        }
    }

    fn fixture() -> (Vec<u8>, Journal) {
        let contexto = fixtures::contexto(b"\n");
        let journal = fixtures::journal_valido(&contexto);
        let envelope = EnvelopeContextoAdmision::ligar(contexto, &journal);
        (super::super::codificar(&envelope).unwrap(), journal)
    }

    fn error(bytes: Vec<u8>, limite: Option<usize>) -> ErrorCodecEnvelopeAdmision {
        let mut reader = ReaderControlado::nuevo(bytes);
        match limite {
            Some(limite) => leer_con_limite(&mut reader, limite),
            None => leer(&mut reader),
        }
        .err()
        .unwrap()
    }

    fn cambiar_total(bytes: &mut [u8], total: u64) {
        bytes[MAGIC.len() + 2..HEADER_BYTES].copy_from_slice(&total.to_be_bytes());
    }

    #[test]
    fn rechaza_header_y_journal_fuera_de_cota_sin_leer_body() {
        let (original, _) = fixture();
        let header_bytes = HEADER_BYTES + JOURNAL_LENGTH_BYTES;

        let mut over_limit = original.clone();
        cambiar_total(&mut over_limit, u64::try_from(MAX_CODEC_BYTES).unwrap() + 1);
        let mut reader = ReaderControlado::nuevo(over_limit);
        reader.panic_despues = Some(header_bytes);
        assert_eq!(
            leer(&mut reader).err().unwrap(),
            ErrorCodecEnvelopeAdmision::TotalExcedeLimite
        );
        assert_eq!(reader.posicion, header_bytes);

        let mut bad_magic = original.clone();
        bad_magic[0] ^= 1;
        let mut reader = ReaderControlado::nuevo(bad_magic);
        reader.panic_despues = Some(header_bytes);
        assert_eq!(
            leer(&mut reader).err().unwrap(),
            ErrorCodecEnvelopeAdmision::FramingInvalido
        );
        assert_eq!(reader.posicion, header_bytes);

        let mut bad_version = original.clone();
        bad_version[MAGIC.len()..MAGIC.len() + 2].copy_from_slice(&(VERSION + 1).to_be_bytes());
        let mut reader = ReaderControlado::nuevo(bad_version);
        reader.panic_despues = Some(header_bytes);
        assert_eq!(
            leer(&mut reader).err().unwrap(),
            ErrorCodecEnvelopeAdmision::VersionNoSoportada
        );
        assert_eq!(reader.posicion, header_bytes);

        let mut oversized_journal = original;
        oversized_journal[HEADER_BYTES..header_bytes]
            .copy_from_slice(&u32::try_from(MAX_JOURNAL_BYTES + 1).unwrap().to_be_bytes());
        let mut reader = ReaderControlado::nuevo(oversized_journal);
        reader.panic_despues = Some(header_bytes);
        assert_eq!(
            leer(&mut reader).err().unwrap(),
            ErrorCodecEnvelopeAdmision::JournalInvalido
        );
        assert_eq!(reader.posicion, header_bytes);
    }

    #[test]
    fn admite_fragmentos_interrumpidos_y_limite_exacta_pequena() {
        let (wire, journal) = fixture();
        let mut reader = ReaderControlado::nuevo(wire.clone());
        reader.max_fragmento = 3;
        reader.interrupciones = 12;
        let envelope = leer_con_limite(&mut reader, wire.len()).unwrap();
        envelope
            .validar_replay(&journal, fixtures::MOTOR, fixtures::POLICY)
            .unwrap();
        assert_eq!(reader.posicion, wire.len());

        let mut too_small = ReaderControlado::nuevo(wire.clone());
        assert_eq!(
            leer_con_limite(&mut too_small, wire.len() - 1)
                .err()
                .unwrap(),
            ErrorCodecEnvelopeAdmision::TotalExcedeLimite
        );
        assert_eq!(too_small.posicion, HEADER_BYTES + JOURNAL_LENGTH_BYTES);
    }

    #[test]
    fn rechaza_body_truncado_y_byte_despues_de_eof_de_frame() {
        let (wire, _) = fixture();

        let mut truncado = wire.clone();
        truncado.pop();
        assert_eq!(
            error(truncado, None),
            ErrorCodecEnvelopeAdmision::FramingInvalido
        );

        let mut trailing = wire;
        trailing.push(0);
        assert_eq!(
            error(trailing, None),
            ErrorCodecEnvelopeAdmision::FramingInvalido
        );
    }

    #[test]
    fn longitud_contexto_se_valida_antes_del_body_y_io_error_se_redacta() {
        let (mut wire, _) = fixture();
        let header_end = HEADER_BYTES + JOURNAL_LENGTH_BYTES;
        let journal_len = usize::try_from(u32::from_be_bytes(
            wire[HEADER_BYTES..header_end].try_into().unwrap(),
        ))
        .unwrap();
        let context_len_start = header_end + journal_len;
        wire[context_len_start..context_len_start + CONTEXT_LENGTH_BYTES]
            .copy_from_slice(&u64::MAX.to_be_bytes());
        let consumed = context_len_start + CONTEXT_LENGTH_BYTES;
        let mut reader = ReaderControlado::nuevo(wire);
        reader.panic_despues = Some(consumed);
        assert_eq!(
            leer(&mut reader).err().unwrap(),
            ErrorCodecEnvelopeAdmision::LongitudExcedida
        );
        assert_eq!(reader.posicion, consumed);

        let (wire, _) = fixture();
        let mut reader = ReaderControlado::nuevo(wire);
        reader.fallar_en = Some(HEADER_BYTES + JOURNAL_LENGTH_BYTES);
        let error = leer(&mut reader).err().unwrap();
        assert_eq!(error.codigo(), "admission_envelope_read_failed");
        assert_eq!(error.to_string(), error.codigo());
        assert!(!format!("{error:?}").contains("secret-path-marker"));
    }
}
