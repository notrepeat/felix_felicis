use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use crate::transaccion::anclado::identidad_archivo::{IdentidadArchivo, identidad_archivo};
use crate::transaccion::anclado::{ScopeFilesystem, scope_filesystem};
use tempfile::{TempDir, tempdir};

use super::{ErrorLecturaFrameIntent, leer_acotado, leer_frame_intent, leer_frame_intent_con_hook};

struct ArchivoFixture {
    _directorio: TempDir,
    ruta: PathBuf,
    archivo: File,
    bytes: Vec<u8>,
    identidad: IdentidadArchivo,
    scope: ScopeFilesystem,
}

fn archivo_fixture(bytes: &[u8]) -> ArchivoFixture {
    let directorio = tempdir().unwrap();
    let ruta = directorio.path().join("intent-frame");
    let mut archivo = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&ruta)
        .unwrap();
    archivo.write_all(bytes).unwrap();
    archivo.seek(SeekFrom::End(0)).unwrap();
    let identidad = identidad_archivo(&archivo).unwrap();
    let scope = scope_filesystem(&archivo).unwrap();
    ArchivoFixture {
        _directorio: directorio,
        ruta,
        archivo,
        bytes: bytes.to_vec(),
        identidad,
        scope,
    }
}

#[test]
fn acepta_el_cap_exacto_y_rebobina_el_handle_original() {
    let fixture = archivo_fixture(b"frame with cursor already at eof");
    let cap = fixture.bytes.len();
    let observado = leer_frame_intent(fixture.archivo, fixture.scope, cap).unwrap();

    assert_eq!(observado.bytes, fixture.bytes);
    assert!(observado.identidad_archivo == fixture.identidad);
    assert!(observado.scope == fixture.scope);
    assert_eq!(
        observado.archivo_original.metadata().unwrap().len(),
        cap as u64
    );
    assert_eq!(format!("{observado:?}"), "FrameIntentLeido { .. }");
    assert_eq!(fs::read(&fixture.ruta).unwrap(), fixture.bytes);
}

#[test]
fn metadata_cap_mas_uno_rechaza_antes_del_hook_y_preserva_bytes() {
    let fixture = archivo_fixture(b"frame");
    let hook_llamado = std::cell::Cell::new(false);
    let error = leer_frame_intent_con_hook(
        fixture.archivo,
        fixture.scope,
        fixture.bytes.len() - 1,
        || hook_llamado.set(true),
    )
    .unwrap_err();

    assert_eq!(error, ErrorLecturaFrameIntent::FrameTooLarge);
    assert!(!hook_llamado.get());
    assert_eq!(fs::read(&fixture.ruta).unwrap(), fixture.bytes);
}

#[test]
fn crecimiento_despues_de_metadata_falla_por_el_byte_sentinela() {
    let fixture = archivo_fixture(b"frame");
    let cap = fixture.bytes.len();
    let mutation = fixture.archivo.try_clone().unwrap();
    let error = leer_frame_intent_con_hook(fixture.archivo, fixture.scope, cap, || {
        mutation.set_len((cap + 1) as u64).unwrap()
    })
    .unwrap_err();

    assert_eq!(error, ErrorLecturaFrameIntent::FrameTooLarge);
    let bytes = fs::read(&fixture.ruta).unwrap();
    assert_eq!(bytes.len(), cap + 1);
    assert_eq!(&bytes[..cap], fixture.bytes);
}

#[test]
fn scope_distinto_del_esperado_se_rechaza_sin_invocar_hook() {
    let fixture = archivo_fixture(b"frame");
    let hook_llamado = std::cell::Cell::new(false);
    let error = leer_frame_intent_con_hook(
        fixture.archivo,
        ScopeFilesystem(fixture.scope.0 ^ 1),
        fixture.bytes.len(),
        || hook_llamado.set(true),
    )
    .unwrap_err();

    assert_eq!(error, ErrorLecturaFrameIntent::ScopeMismatch);
    assert!(!hook_llamado.get());
    assert_eq!(fs::read(&fixture.ruta).unwrap(), fixture.bytes);
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
        Err(io::Error::other("private file path and contents"))
    }
}

#[test]
fn colector_reintenta_interrupted_y_redacta_error_de_lectura() {
    let expected = b"fragmented intent frame";
    let cap_mas_uno = u64::try_from(expected.len() + 1).unwrap();
    let mut interrupted = InterruptedOnce {
        cursor: Cursor::new(expected),
        interrupted: false,
    };
    let bytes = leer_acotado(&mut interrupted, expected.len(), cap_mas_uno).unwrap();
    assert_eq!(bytes, expected);

    let error = leer_acotado(&mut FailingReader, 8, 9).unwrap_err();
    assert_eq!(error, ErrorLecturaFrameIntent::ReadIo);
    assert!(!format!("{error:?}").contains("private file path"));
}

#[test]
fn cap_sin_capacidad_para_el_sentinela_falla_antes_de_leer() {
    let fixture = archivo_fixture(b"frame");
    let error = leer_frame_intent(fixture.archivo, fixture.scope, usize::MAX).unwrap_err();

    assert_eq!(error, ErrorLecturaFrameIntent::LengthOverflow);
    assert_eq!(fs::read(&fixture.ruta).unwrap(), fixture.bytes);
}
