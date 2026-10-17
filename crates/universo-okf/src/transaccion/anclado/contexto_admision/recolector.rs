//! Recolector acotado para frames versionados; no interpreta su contenido.

use std::fmt;
use std::io::{self, Read};

use sha2::{Digest, Sha256};

/// Fallo fijo del recolector; nunca conserva mensajes de I/O o asignación.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorRecoleccionFrame {
    TooLarge,
    Overflow,
    Reserve,
    Io,
}

impl ErrorRecoleccionFrame {
    const fn codigo(self) -> &'static str {
        match self {
            Self::TooLarge => "context_frame_too_large",
            Self::Overflow => "context_frame_length_overflow",
            Self::Reserve => "context_frame_reserve_failed",
            Self::Io => "context_frame_io",
        }
    }
}

impl fmt::Debug for ErrorRecoleccionFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorRecoleccionFrame")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorRecoleccionFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorRecoleccionFrame {}

pub(super) struct FrameReunido {
    pub(super) bytes: Vec<u8>,
    pub(super) longitud: u64,
    pub(super) sha256: [u8; 32],
}

impl fmt::Debug for FrameReunido {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FrameReunido([redacted])")
    }
}

/// Reúne como máximo cap bytes; el byte adicional solo detecta exceso.
pub(super) fn reunir<R: Read>(
    reader: &mut R,
    cap: usize,
) -> Result<FrameReunido, ErrorRecoleccionFrame> {
    let limite = cap.checked_add(1).ok_or(ErrorRecoleccionFrame::Overflow)?;
    let limite_u64 = u64::try_from(limite).map_err(|_| ErrorRecoleccionFrame::Overflow)?;
    let mut bounded = reader.take(limite_u64);
    let mut bytes = Vec::new();
    let mut longitud = 0_u64;
    let mut digest = Sha256::new();
    let mut fragmento = [0_u8; 1024];

    loop {
        let leidos = match bounded.read(&mut fragmento) {
            Ok(0) => break,
            Ok(leidos) => leidos,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorRecoleccionFrame::Io),
        };

        let longitud_siguiente = bytes
            .len()
            .checked_add(leidos)
            .ok_or(ErrorRecoleccionFrame::Overflow)?;
        if longitud_siguiente > cap {
            return Err(ErrorRecoleccionFrame::TooLarge);
        }
        let incremento = u64::try_from(leidos).map_err(|_| ErrorRecoleccionFrame::Overflow)?;
        longitud = longitud
            .checked_add(incremento)
            .ok_or(ErrorRecoleccionFrame::Overflow)?;

        bytes
            .try_reserve(leidos)
            .map_err(|_| ErrorRecoleccionFrame::Reserve)?;
        bytes.extend_from_slice(&fragmento[..leidos]);
        digest.update(&fragmento[..leidos]);
    }

    Ok(FrameReunido {
        bytes,
        longitud,
        sha256: digest.finalize().into(),
    })
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, Read};

    use sha2::{Digest, Sha256};

    use super::{ErrorRecoleccionFrame, reunir};

    struct FragmentedReader<'a> {
        cursor: Cursor<&'a [u8]>,
        fragment_limit: usize,
    }

    impl Read for FragmentedReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let limit = buffer.len().min(self.fragment_limit);
            self.cursor.read(&mut buffer[..limit])
        }
    }

    struct InterruptedOnce<'a> {
        cursor: Cursor<&'a [u8]>,
        interrupted: bool,
    }

    impl Read for InterruptedOnce<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.cursor.read(buffer)
        }
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("private source detail"))
        }
    }

    #[test]
    fn fragmented_reads_reassemble_the_frame() {
        let expected = b"frame split across short reads";
        let mut reader = FragmentedReader {
            cursor: Cursor::new(expected),
            fragment_limit: 3,
        };

        let frame = reunir(&mut reader, expected.len()).unwrap();

        assert_eq!(frame.bytes, expected);
        assert_eq!(frame.longitud, u64::try_from(expected.len()).unwrap());
    }

    #[test]
    fn exact_cap_reaches_eof_successfully() {
        let expected = b"exact";
        let mut reader = Cursor::new(expected);

        let frame = reunir(&mut reader, expected.len()).unwrap();

        assert_eq!(frame.bytes, expected);
        assert_eq!(frame.longitud, u64::try_from(expected.len()).unwrap());
    }

    #[test]
    fn cap_plus_one_is_rejected() {
        let mut reader = Cursor::new(b"frame");

        let error = reunir(&mut reader, 4).unwrap_err();

        assert_eq!(error, ErrorRecoleccionFrame::TooLarge);
    }

    #[test]
    fn interrupted_read_is_retried() {
        let expected = b"interrupted once";
        let mut reader = InterruptedOnce {
            cursor: Cursor::new(expected),
            interrupted: false,
        };

        let frame = reunir(&mut reader, expected.len()).unwrap();

        assert!(reader.interrupted);
        assert_eq!(frame.bytes, expected);
    }

    #[test]
    fn io_error_is_fixed_and_redacted() {
        let mut reader = FailingReader;
        let error = reunir(&mut reader, 8).unwrap_err();
        let rendered = format!("{error:?} {error}");

        assert_eq!(error, ErrorRecoleccionFrame::Io);
        assert!(rendered.contains("context_frame_io"));
        assert!(!rendered.contains("private source detail"));
    }

    #[test]
    fn digest_covers_exactly_the_returned_bytes() {
        let expected = b"hash these exact bytes";
        let mut reader = Cursor::new(expected);

        let frame = reunir(&mut reader, expected.len()).unwrap();

        assert_eq!(frame.bytes, expected);
        let digest_esperado: [u8; 32] = Sha256::digest(expected).into();
        assert_eq!(frame.sha256, digest_esperado);
        assert_eq!(frame.longitud, u64::try_from(expected.len()).unwrap());
    }

    #[test]
    fn cap_overflow_fails_before_reading() {
        let mut reader = FailingReader;

        let error = reunir(&mut reader, usize::MAX).unwrap_err();

        assert_eq!(error, ErrorRecoleccionFrame::Overflow);
    }
}
