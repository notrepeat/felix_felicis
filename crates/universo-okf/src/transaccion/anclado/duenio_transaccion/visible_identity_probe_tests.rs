use super::*;
use crate::transaccion::anclado::DirectorioAnclado;
use crate::transaccion::raiz::UbicacionRaiz;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const FILE_BYTES: &[u8] = b"visible identity probe canary";

struct Fixture {
    _temp: TempDir,
    parent_path: PathBuf,
    directorio: DirectorioAnclado,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    fs::create_dir(&parent_path).unwrap();
    let ubicacion = UbicacionRaiz::nueva(&parent_path.join("bundle")).unwrap();
    let directorio = DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap();
    Fixture {
        _temp: temp,
        parent_path,
        directorio,
    }
}

fn scope_real(fixture: &Fixture) -> ScopeFilesystem {
    scope_filesystem(&fixture.directorio.parent).unwrap()
}

fn identidad_de(path: &Path) -> IdentidadArchivo {
    identidad_archivo(&File::open(path).unwrap()).unwrap()
}

#[test]
fn mismo_archivo_regular_pasa_y_conserva_nombre_bytes_e_identidad() {
    let fixture = fixture();
    let nombre = std::ffi::OsStr::new("candidate.journal");
    let path = fixture.parent_path.join(nombre);
    fs::write(&path, FILE_BYTES).unwrap();
    let retenido = File::open(&path).unwrap();
    let identidad_esperada = identidad_archivo(&retenido).unwrap();

    confirmar_sibling_presente_con_identidad(
        &fixture.directorio.parent,
        nombre,
        scope_real(&fixture),
        identidad_esperada,
    )
    .unwrap();

    assert_eq!(fs::read(&path).unwrap(), FILE_BYTES);
    assert_eq!(identidad_de(&path), identidad_esperada);
    assert_eq!(identidad_archivo(&retenido).unwrap(), identidad_esperada);
}

#[test]
fn reemplazo_con_bytes_iguales_e_identidad_distinta_falla_y_preserva_ambos() {
    let fixture = fixture();
    let nombre = std::ffi::OsStr::new("candidate.journal");
    let path = fixture.parent_path.join(nombre);
    let retenido_path = fixture.parent_path.join("candidate.retained");
    let reemplazo_path = fixture.parent_path.join("replacement.source");
    fs::write(&path, FILE_BYTES).unwrap();
    fs::write(&reemplazo_path, FILE_BYTES).unwrap();
    let retenido = File::open(&path).unwrap();
    let reemplazo_retenido = File::open(&reemplazo_path).unwrap();
    let identidad_original = identidad_archivo(&retenido).unwrap();
    let identidad_reemplazo = identidad_archivo(&reemplazo_retenido).unwrap();
    assert_ne!(identidad_original, identidad_reemplazo);

    fs::rename(&path, &retenido_path).unwrap();
    fs::rename(&reemplazo_path, &path).unwrap();
    let resultado = confirmar_sibling_presente_con_identidad(
        &fixture.directorio.parent,
        nombre,
        scope_real(&fixture),
        identidad_original,
    );

    assert!(matches!(
        resultado,
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));
    assert_eq!(fs::read(&retenido_path).unwrap(), FILE_BYTES);
    assert_eq!(fs::read(&path).unwrap(), FILE_BYTES);
    assert_eq!(identidad_de(&retenido_path), identidad_original);
    assert_eq!(identidad_de(&path), identidad_reemplazo);
    assert_eq!(identidad_archivo(&retenido).unwrap(), identidad_original);
    assert_eq!(
        identidad_archivo(&reemplazo_retenido).unwrap(),
        identidad_reemplazo
    );
}

#[test]
fn nombre_ausente_falla_cerrado_y_preserva_archivo_de_referencia() {
    let fixture = fixture();
    let referencia = fixture.parent_path.join("identity.source");
    fs::write(&referencia, FILE_BYTES).unwrap();
    let identidad_referencia = identidad_de(&referencia);

    let resultado = confirmar_sibling_presente_con_identidad(
        &fixture.directorio.parent,
        std::ffi::OsStr::new("missing.journal"),
        scope_real(&fixture),
        identidad_referencia,
    );

    assert!(matches!(
        resultado,
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));
    assert!(!fixture.parent_path.join("missing.journal").exists());
    assert_eq!(fs::read(&referencia).unwrap(), FILE_BYTES);
    assert_eq!(identidad_de(&referencia), identidad_referencia);
}

#[test]
fn directorio_falla_cerrado_y_preserva_canary_y_referencia() {
    let fixture = fixture();
    let directory = fixture.parent_path.join("candidate.journal");
    let referencia = fixture.parent_path.join("identity.source");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("canary"), FILE_BYTES).unwrap();
    fs::write(&referencia, FILE_BYTES).unwrap();
    let identidad_referencia = identidad_de(&referencia);

    let resultado = confirmar_sibling_presente_con_identidad(
        &fixture.directorio.parent,
        std::ffi::OsStr::new("candidate.journal"),
        scope_real(&fixture),
        identidad_referencia,
    );

    assert!(matches!(
        resultado,
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));
    assert_eq!(fs::read(directory.join("canary")).unwrap(), FILE_BYTES);
    assert_eq!(fs::read(&referencia).unwrap(), FILE_BYTES);
    assert_eq!(identidad_de(&referencia), identidad_referencia);
}

#[test]
fn error_de_scope_en_hook_privado_falla_cerrado_y_preserva_archivo() {
    let fixture = fixture();
    let nombre = std::ffi::OsStr::new("candidate.journal");
    let path = fixture.parent_path.join(nombre);
    fs::write(&path, FILE_BYTES).unwrap();
    let identidad_esperada = identidad_de(&path);

    let resultado = confirmar_sibling_con_scope_de_prueba(
        &fixture.directorio.parent,
        nombre,
        scope_real(&fixture),
        identidad_esperada,
        |_| Err(ErrorDirectorioAnclado::Io),
    );

    assert!(matches!(
        resultado,
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));
    assert_eq!(fs::read(&path).unwrap(), FILE_BYTES);
    assert_eq!(identidad_de(&path), identidad_esperada);
}

#[test]
fn scope_inyectado_distinto_falla_cerrado_y_preserva_archivo() {
    let fixture = fixture();
    let nombre = std::ffi::OsStr::new("candidate.journal");
    let path = fixture.parent_path.join(nombre);
    fs::write(&path, FILE_BYTES).unwrap();
    let identidad_esperada = identidad_de(&path);
    let scope_esperado = scope_real(&fixture);
    let scope_inyectado = ScopeFilesystem(scope_esperado.0.wrapping_add(1));

    let resultado = confirmar_sibling_con_scope_de_prueba(
        &fixture.directorio.parent,
        nombre,
        scope_esperado,
        identidad_esperada,
        |_| Ok(scope_inyectado),
    );

    assert!(matches!(
        resultado,
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));
    assert_eq!(fs::read(&path).unwrap(), FILE_BYTES);
    assert_eq!(identidad_de(&path), identidad_esperada);
}
