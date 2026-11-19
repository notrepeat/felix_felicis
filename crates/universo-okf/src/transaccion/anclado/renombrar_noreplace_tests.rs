use super::{CausaRenombrarNoReemplazo, renombrar_unix, renombrar_unix_con};
use std::cell::Cell;
use std::ffi::CString;
use std::fs::File;

#[test]
fn archivo_regular_no_reemplaza_destino_y_luego_puede_moverse() {
    let temp = tempfile::tempdir().unwrap();
    let fuente = temp.path().join("temporal");
    let destino = temp.path().join("canonico");
    std::fs::write(&fuente, b"temporal canary").unwrap();
    std::fs::write(&destino, b"canonical canary").unwrap();
    let parent = File::open(temp.path()).unwrap();
    let fuente_nombre = CString::new("temporal").unwrap();
    let destino_nombre = CString::new("canonico").unwrap();

    let error = renombrar_unix(&parent, &fuente_nombre, &destino_nombre)
        .expect_err("no-replace rename must reject an existing sibling");
    assert!(matches!(
        error.causa(),
        CausaRenombrarNoReemplazo::DestinoExistente
    ));
    assert_eq!(std::fs::read(&fuente).unwrap(), b"temporal canary");
    assert_eq!(std::fs::read(&destino).unwrap(), b"canonical canary");

    std::fs::remove_file(&destino).unwrap();
    assert!(renombrar_unix(&parent, &fuente_nombre, &destino_nombre).is_ok());
    assert!(!fuente.exists());
    assert_eq!(std::fs::read(&destino).unwrap(), b"temporal canary");
}

#[test]
fn renameat2_no_soportado_falla_cerrado_sin_reintento_ni_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let fuente = temp.path().join("temporal");
    let destino = temp.path().join("canonico");
    std::fs::write(&fuente, b"temporal canary").unwrap();
    let parent = File::open(temp.path()).unwrap();
    let fuente_nombre = CString::new("temporal").unwrap();
    let destino_nombre = CString::new("canonico").unwrap();
    let llamadas = Cell::new(0);

    let resultado = renombrar_unix_con(&parent, &fuente_nombre, &destino_nombre, |_, _, _| {
        llamadas.set(llamadas.get() + 1);
        (-1, Some(libc::ENOSYS))
    });

    let error = match resultado {
        Err(error) => error,
        Ok(()) => panic!("an unavailable renameat2 must fail closed"),
    };
    assert!(matches!(error.causa(), CausaRenombrarNoReemplazo::Io));
    assert_eq!(llamadas.get(), 1);
    assert_eq!(std::fs::read(&fuente).unwrap(), b"temporal canary");
    assert!(!destino.exists());
}
