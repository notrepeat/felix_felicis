//! Evidencia de bytes observados durante la decodificación bounded del frame.

use std::fmt;
use std::io::{self, Read};

use sha2::{Digest, Sha256};

use crate::transaccion::admision::envelope::{EnvelopeContextoAdmision, codec};

use super::super::IdentidadArchivo;

/// Datos de una lectura concreta; no autentican el contexto ni dan autoridad.
pub(crate) struct EvidenciaLecturaContexto {
    pub(crate) identidad_archivo: IdentidadArchivo,
    pub(crate) longitud: u64,
    pub(crate) sha256: [u8; 32],
}

impl fmt::Debug for EvidenciaLecturaContexto {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EvidenciaLecturaContexto { .. }")
    }
}

pub(super) fn leer<R: Read>(
    reader: &mut R,
    identidad_archivo: IdentidadArchivo,
) -> Result<(EnvelopeContextoAdmision, EvidenciaLecturaContexto), codec::ErrorCodecEnvelopeAdmision>
{
    let mut lector = LectorConDigest {
        reader,
        digest: Sha256::new(),
        longitud: 0,
    };
    let envelope = codec::leer(&mut lector)?;
    let evidencia = lector.terminar(identidad_archivo);
    Ok((envelope, evidencia))
}

struct LectorConDigest<'reader, R> {
    reader: &'reader mut R,
    digest: Sha256,
    longitud: u64,
}

impl<R: Read> Read for LectorConDigest<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let leidos = self.reader.read(buffer)?;
        let incremento =
            u64::try_from(leidos).map_err(|_| io::Error::other("context frame length overflow"))?;
        self.longitud = self
            .longitud
            .checked_add(incremento)
            .ok_or_else(|| io::Error::other("context frame length overflow"))?;
        self.digest.update(&buffer[..leidos]);
        Ok(leidos)
    }
}

impl<R> LectorConDigest<'_, R> {
    fn terminar(self, identidad_archivo: IdentidadArchivo) -> EvidenciaLecturaContexto {
        EvidenciaLecturaContexto {
            identidad_archivo,
            longitud: self.longitud,
            sha256: self.digest.finalize().into(),
        }
    }
}
