//! Codec de una identidad física de archivo representada como datos.
//!
//! Este módulo es hijo de `identidad_archivo`: puede copiar sus campos nativos
//! a un DTO, pero deliberadamente no contiene la conversión inversa.

use std::fmt;
use std::io::Read;

use serde::{Deserialize, Serialize};

use super::IdentidadArchivo;

const MAGIC: &[u8] = b"FELIX-FILEID\0";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = MAGIC.len() + 2 + 4;
const MAX_PAYLOAD_BYTES: usize = 128;
const MAX_FRAME_BYTES: usize = HEADER_BYTES + MAX_PAYLOAD_BYTES;

/// Los valores persistidos son comparadores, no objetos de filesystem.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct IdentidadArchivoPersistida(DatosIdentidadArchivo);

#[derive(Clone, PartialEq, Eq)]
enum DatosIdentidadArchivo {
    Unix { device: u64, inode: u64 },
    Windows { volume: u64, file_id: [u8; 16] },
}

impl fmt::Debug for IdentidadArchivoPersistida {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IdentidadArchivoPersistida([redacted])")
    }
}

/// Error fijo que no incluye bytes, identidades ni detalles de serde o I/O.
pub(crate) struct ErrorIdentidadArchivoPersistida;

impl fmt::Debug for ErrorIdentidadArchivoPersistida {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ErrorIdentidadArchivoPersistida")
    }
}

impl fmt::Display for ErrorIdentidadArchivoPersistida {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid_persisted_file_identity")
    }
}

impl std::error::Error for ErrorIdentidadArchivoPersistida {}

impl TryFrom<IdentidadArchivo> for IdentidadArchivoPersistida {
    type Error = ErrorIdentidadArchivoPersistida;

    /// Copia la identidad que ya se observó en un handle; no hace I/O.
    fn try_from(identity: IdentidadArchivo) -> Result<Self, Self::Error> {
        #[cfg(unix)]
        {
            Ok(Self(DatosIdentidadArchivo::Unix {
                device: identity.dispositivo,
                inode: identity.inode,
            }))
        }
        #[cfg(windows)]
        {
            if identidad_windows_sentinel(&identity.id_archivo) {
                return Err(ErrorIdentidadArchivoPersistida);
            }
            Ok(Self(DatosIdentidadArchivo::Windows {
                volume: identity.volumen,
                file_id: identity.id_archivo,
            }))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = identity;
            Err(ErrorIdentidadArchivoPersistida)
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "platform", deny_unknown_fields)]
enum IdentidadArchivoWire {
    #[serde(rename = "unix-v1")]
    Unix { device: u64, inode: u64 },
    #[serde(rename = "windows-v1")]
    Windows { volume: u64, file_id: String },
}

impl IdentidadArchivoPersistida {
    pub(crate) fn encode(&self) -> Result<Vec<u8>, ErrorIdentidadArchivoPersistida> {
        let wire = match &self.0 {
            DatosIdentidadArchivo::Unix { device, inode } => IdentidadArchivoWire::Unix {
                device: *device,
                inode: *inode,
            },
            DatosIdentidadArchivo::Windows { volume, file_id } => {
                if identidad_windows_sentinel(file_id) {
                    return Err(ErrorIdentidadArchivoPersistida);
                }
                IdentidadArchivoWire::Windows {
                    volume: *volume,
                    file_id: file_id_hex(file_id),
                }
            }
        };
        let payload = serde_json::to_vec(&wire).map_err(|_| ErrorIdentidadArchivoPersistida)?;
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(ErrorIdentidadArchivoPersistida);
        }

        let length = u32::try_from(payload.len()).map_err(|_| ErrorIdentidadArchivoPersistida)?;
        let mut frame = Vec::with_capacity(HEADER_BYTES + payload.len());
        frame.extend_from_slice(MAGIC);
        frame.extend_from_slice(&VERSION.to_be_bytes());
        frame.extend_from_slice(&length.to_be_bytes());
        frame.extend_from_slice(&payload);
        Ok(frame)
    }

    /// Decodifica un único frame acotado y exige EOF después de su payload.
    pub(crate) fn decode<R: Read>(reader: &mut R) -> Result<Self, ErrorIdentidadArchivoPersistida> {
        let mut header = [0_u8; HEADER_BYTES];
        reader
            .read_exact(&mut header)
            .map_err(|_| ErrorIdentidadArchivoPersistida)?;
        if &header[..MAGIC.len()] != MAGIC {
            return Err(ErrorIdentidadArchivoPersistida);
        }

        let version_start = MAGIC.len();
        let version_end = version_start + 2;
        let version = u16::from_be_bytes(
            header[version_start..version_end]
                .try_into()
                .map_err(|_| ErrorIdentidadArchivoPersistida)?,
        );
        if version != VERSION {
            return Err(ErrorIdentidadArchivoPersistida);
        }

        let payload_length = u32::from_be_bytes(
            header[version_end..]
                .try_into()
                .map_err(|_| ErrorIdentidadArchivoPersistida)?,
        );
        let payload_length =
            usize::try_from(payload_length).map_err(|_| ErrorIdentidadArchivoPersistida)?;
        if payload_length > MAX_PAYLOAD_BYTES || HEADER_BYTES + payload_length > MAX_FRAME_BYTES {
            return Err(ErrorIdentidadArchivoPersistida);
        }

        let mut payload = vec![0_u8; payload_length];
        reader
            .read_exact(&mut payload)
            .map_err(|_| ErrorIdentidadArchivoPersistida)?;
        exigir_eof(reader)?;

        let wire: IdentidadArchivoWire =
            serde_json::from_slice(&payload).map_err(|_| ErrorIdentidadArchivoPersistida)?;
        let identidad = Self::desde_wire(wire)?;
        let canonico = identidad.encode()?;
        if canonico.len() != HEADER_BYTES + payload.len()
            || &canonico[..HEADER_BYTES] != header.as_slice()
            || &canonico[HEADER_BYTES..] != payload.as_slice()
        {
            return Err(ErrorIdentidadArchivoPersistida);
        }
        Ok(identidad)
    }

    fn desde_wire(wire: IdentidadArchivoWire) -> Result<Self, ErrorIdentidadArchivoPersistida> {
        match wire {
            IdentidadArchivoWire::Unix { device, inode } => {
                Ok(Self(DatosIdentidadArchivo::Unix { device, inode }))
            }
            IdentidadArchivoWire::Windows { volume, file_id } => {
                let file_id = file_id_desde_hex(&file_id)?;
                if identidad_windows_sentinel(&file_id) {
                    return Err(ErrorIdentidadArchivoPersistida);
                }
                Ok(Self(DatosIdentidadArchivo::Windows { volume, file_id }))
            }
        }
    }

    #[cfg(test)]
    fn sintetica_unix(device: u64, inode: u64) -> Self {
        Self(DatosIdentidadArchivo::Unix { device, inode })
    }

    #[cfg(test)]
    fn sintetica_windows(volume: u64, file_id: [u8; 16]) -> Self {
        Self(DatosIdentidadArchivo::Windows { volume, file_id })
    }
}

fn exigir_eof<R: Read>(reader: &mut R) -> Result<(), ErrorIdentidadArchivoPersistida> {
    let mut byte = [0_u8; 1];
    loop {
        match reader.read(&mut byte) {
            Ok(0) => return Ok(()),
            Ok(_) => return Err(ErrorIdentidadArchivoPersistida),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorIdentidadArchivoPersistida),
        }
    }
}

fn file_id_desde_hex(text: &str) -> Result<[u8; 16], ErrorIdentidadArchivoPersistida> {
    let bytes = text.as_bytes();
    if bytes.len() != 32
        || bytes
            .iter()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(byte))
    {
        return Err(ErrorIdentidadArchivoPersistida);
    }

    let mut file_id = [0_u8; 16];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        file_id[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Ok(file_id)
}

fn hex_nibble(byte: u8) -> Result<u8, ErrorIdentidadArchivoPersistida> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(ErrorIdentidadArchivoPersistida),
    }
}

fn file_id_hex(file_id: &[u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(32);
    for byte in file_id {
        text.push(HEX[(byte >> 4) as usize] as char);
        text.push(HEX[(byte & 0x0f) as usize] as char);
    }
    text
}

fn identidad_windows_sentinel(file_id: &[u8; 16]) -> bool {
    file_id.iter().all(|byte| *byte == 0) || file_id.iter().all(|byte| *byte == u8::MAX)
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::io::Cursor;

    use super::super::{IdentidadArchivo, identidad_archivo};
    use super::{
        ErrorIdentidadArchivoPersistida, IdentidadArchivoPersistida, MAGIC, MAX_FRAME_BYTES,
        MAX_PAYLOAD_BYTES, VERSION,
    };

    fn frame(payload: &[u8], version: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&version.to_be_bytes());
        bytes.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn windows_payload(file_id: &str) -> Vec<u8> {
        format!("{{\"platform\":\"windows-v1\",\"volume\":9,\"file_id\":\"{file_id}\"}}")
            .into_bytes()
    }

    fn assert_rejected(bytes: &[u8]) {
        let mut input = Cursor::new(bytes);
        assert!(IdentidadArchivoPersistida::decode(&mut input).is_err());
    }

    #[test]
    fn roundtrip_sintetico_unix_preserva_ceros_permitidos() {
        let identity = IdentidadArchivoPersistida::sintetica_unix(0, u64::MAX);
        let frame = identity.encode().unwrap();
        let mut input = Cursor::new(frame);
        assert_eq!(
            IdentidadArchivoPersistida::decode(&mut input).unwrap(),
            identity
        );
    }

    #[test]
    fn roundtrip_sintetico_windows_usa_file_id_hex_minuscula() {
        let mut file_id = [0_u8; 16];
        for (index, byte) in file_id.iter_mut().enumerate() {
            *byte = u8::try_from(index + 1).unwrap();
        }
        let identity = IdentidadArchivoPersistida::sintetica_windows(7, file_id);
        let frame = identity.encode().unwrap();
        let mut input = Cursor::new(frame);
        assert_eq!(
            IdentidadArchivoPersistida::decode(&mut input).unwrap(),
            identity
        );
    }

    #[test]
    fn conversion_desde_handle_real_es_unidireccional_y_redactada() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("identity-canary");
        fs::write(&path, b"identity payload canary").unwrap();
        let handle = File::open(path).unwrap();
        let native: IdentidadArchivo = identidad_archivo(&handle).unwrap();
        let persisted = IdentidadArchivoPersistida::try_from(native).unwrap();
        let mut input = Cursor::new(persisted.encode().unwrap());
        assert_eq!(
            IdentidadArchivoPersistida::decode(&mut input).unwrap(),
            persisted
        );
        assert_eq!(
            format!("{persisted:?}"),
            "IdentidadArchivoPersistida([redacted])"
        );
    }

    #[test]
    fn rechaza_version_plataforma_campos_y_ids_windows_invalidos() {
        let payload = b"{\"platform\":\"unix-v1\",\"device\":1,\"inode\":2}";
        assert_rejected(&frame(payload, VERSION + 1));
        assert_rejected(&frame(b"{\"platform\":\"other-v1\"}", VERSION));
        assert_rejected(&frame(
            b"{\"platform\":\"unix-v1\",\"device\":1,\"inode\":2,\"extra\":0}",
            VERSION,
        ));
        assert_rejected(&frame(
            &windows_payload("0000000000000000000000000000000A"),
            VERSION,
        ));
        assert_rejected(&frame(
            &windows_payload("0000000000000000000000000000000"),
            VERSION,
        ));
        assert_rejected(&frame(
            &windows_payload("00000000000000000000000000000000"),
            VERSION,
        ));
        assert_rejected(&frame(
            &windows_payload("ffffffffffffffffffffffffffffffff"),
            VERSION,
        ));
    }

    #[test]
    fn rechaza_truncado_exceso_de_cuota_bytes_extra_y_json_no_canonico() {
        let valid = IdentidadArchivoPersistida::sintetica_unix(1, 2)
            .encode()
            .unwrap();
        assert_rejected(&valid[..valid.len() - 1]);

        let mut oversized = Vec::new();
        oversized.extend_from_slice(MAGIC);
        oversized.extend_from_slice(&VERSION.to_be_bytes());
        oversized.extend_from_slice(&u32::try_from(MAX_PAYLOAD_BYTES + 1).unwrap().to_be_bytes());
        assert!(oversized.len() < MAX_FRAME_BYTES);
        assert_rejected(&oversized);

        let mut with_extra = valid.clone();
        with_extra.push(0);
        assert_rejected(&with_extra);

        let non_canonical = frame(
            b"{ \"platform\":\"unix-v1\",\"device\":1,\"inode\":2 }",
            VERSION,
        );
        assert_rejected(&non_canonical);
    }

    #[test]
    fn error_no_revela_detalles_del_frame() {
        let error = ErrorIdentidadArchivoPersistida;
        assert_eq!(format!("{error:?}"), "ErrorIdentidadArchivoPersistida");
        assert_eq!(error.to_string(), "invalid_persisted_file_identity");
    }
}
