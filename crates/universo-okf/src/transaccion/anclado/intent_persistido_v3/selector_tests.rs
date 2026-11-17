use super::{ErrorSeleccion, IntentSeleccionado, leer};
use crate::transaccion::anclado::intent_persistido::IntentRetiroPersistidoVersionado;
use crate::transaccion::anclado::intent_persistido_v3::{
    IntentRetiroPersistidoV3, tests::committed_fixture,
};
use std::io::{Cursor, Read};

#[test]
fn selecciona_v2_y_v3_sin_cambiar_bytes() {
    let (_, context, metadata) = committed_fixture();
    let legacy = metadata.encode().unwrap();
    let selected = leer(&mut Cursor::new(&legacy)).unwrap();
    let IntentSeleccionado::Legacy(IntentRetiroPersistidoVersionado::V2(decoded)) = selected else {
        panic!("rama incorrecta");
    };
    assert_eq!(decoded.encode().unwrap(), legacy);
    let frame = IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap();
    let IntentSeleccionado::V3(decoded) = leer(&mut Cursor::new(&frame)).unwrap() else {
        panic!("rama incorrecta");
    };
    assert_eq!(decoded.contexto_original(), context);
}

#[test]
fn version_desconocida_falla_antes_del_payload() {
    let (_, context, metadata) = committed_fixture();
    let mut frame = IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap();
    frame[13..15].copy_from_slice(&99_u16.to_be_bytes());
    let mut reader = Cursor::new(frame);
    assert!(matches!(leer(&mut reader), Err(ErrorSeleccion::Version)));
    assert_eq!(reader.position(), 15);
}

#[test]
fn decoder_v3_corrupto_no_reintenta_como_legacy() {
    let (_, context, metadata) = committed_fixture();
    let mut frame = IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap();
    frame[15] = 255;
    assert!(matches!(
        leer(&mut Cursor::new(frame)),
        Err(ErrorSeleccion::V3(_))
    ));
}

#[test]
fn todas_las_truncaciones_y_trailing_fallan() {
    let (_, context, metadata) = committed_fixture();
    let frame = IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap();
    for length in 0..frame.len() {
        assert!(
            leer(&mut Cursor::new(&frame[..length])).is_err(),
            "{length}"
        );
    }
    let mut extra = frame;
    extra.push(0);
    assert!(leer(&mut Cursor::new(extra)).is_err());
    let mut legacy = metadata.encode().unwrap();
    legacy.push(0);
    assert!(leer(&mut Cursor::new(legacy)).is_err());
}

#[test]
fn lectura_fragmentada_con_interrupted_y_error_io() {
    struct Fragmentado<'a> {
        bytes: &'a [u8],
        interrupted: bool,
    }
    impl Read for Fragmentado<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            let count = buf.len().min(self.bytes.len()).min(3);
            buf[..count].copy_from_slice(&self.bytes[..count]);
            self.bytes = &self.bytes[count..];
            Ok(count)
        }
    }
    let (_, context, metadata) = committed_fixture();
    let frame = IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap();
    assert!(
        leer(&mut Fragmentado {
            bytes: &frame,
            interrupted: false
        })
        .is_ok()
    );
    struct Fallo;
    impl Read for Fallo {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "PRIVATE_CANARY",
            ))
        }
    }
    let err = leer(&mut Fallo).err().unwrap();
    assert!(!format!("{err:?}").contains("PRIVATE_CANARY"));
}

#[test]
fn cuotas_legacy_no_se_amplian_por_soportar_v3() {
    use crate::transaccion::anclado::intent_persistido::MAX_INTENT_BYTES;
    use crate::transaccion::anclado::intent_persistido_v2::MAX_RETIRE_V2_BYTES;
    for (version, limit) in [(1_u16, MAX_INTENT_BYTES), (2, MAX_RETIRE_V2_BYTES)] {
        let mut frame = vec![0; limit + 100];
        frame[..13].copy_from_slice(b"FELIX-RETIRE\0");
        frame[13..15].copy_from_slice(&version.to_be_bytes());
        let mut reader = Cursor::new(frame);
        assert!(matches!(leer(&mut reader), Err(ErrorSeleccion::TooLarge)));
        assert_eq!(reader.position(), (limit + 1) as u64);
    }
}
