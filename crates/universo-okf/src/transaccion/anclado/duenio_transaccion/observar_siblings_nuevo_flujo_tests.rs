use super::*;
use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

const LOCK_BYTES: &[u8] = b"sibling probe lock";

struct Fixture {
    _temp: TempDir,
    parent: PathBuf,
    ubicacion: UbicacionRaiz,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("parent");
    fs::create_dir(&parent).unwrap();
    let root = parent.join("bundle");
    let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
    fs::write(ubicacion.lock().as_path(), LOCK_BYTES).unwrap();
    Fixture {
        _temp: temp,
        parent,
        ubicacion,
    }
}

#[test]
fn solo_ausencia_observada_o_archivo_regular_dan_resultado() {
    let fixture = fixture();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let nombre = std::ffi::OsStr::new("candidate.journal");

    assert_eq!(
        observar_sibling(&owner.parent, nombre, owner.scope_parent).unwrap(),
        PresenciaSiblingNuevoFlujo::Ausente
    );
    let path = fixture.parent.join(nombre);
    fs::write(&path, b"regular sibling canary").unwrap();
    assert_eq!(
        observar_sibling(&owner.parent, nombre, owner.scope_parent).unwrap(),
        PresenciaSiblingNuevoFlujo::Presente
    );
    assert_eq!(fs::read(path).unwrap(), b"regular sibling canary");
}

#[test]
fn directorio_en_nombre_sibling_falla_cerrado_y_se_preserva() {
    let fixture = fixture();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let path = fixture.parent.join("candidate.journal");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("canary"), b"directory canary").unwrap();

    let resultado = observar_sibling(&owner.parent, path.file_name().unwrap(), owner.scope_parent);

    assert!(matches!(
        resultado,
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));
    assert_eq!(fs::read(path.join("canary")).unwrap(), b"directory canary");
}

#[cfg(unix)]
#[test]
fn enlace_sibling_falla_cerrado_sin_tocar_link_ni_target() {
    use std::os::unix::fs::symlink;

    let fixture = fixture();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let target = fixture.parent.join("target-canary");
    let link = fixture.parent.join("candidate.journal");
    fs::write(&target, b"symlink target canary").unwrap();
    symlink(&target, &link).unwrap();

    let resultado = observar_sibling(&owner.parent, link.file_name().unwrap(), owner.scope_parent);

    assert!(matches!(
        resultado,
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(target).unwrap(), b"symlink target canary");
}

#[test]
fn scope_distinto_del_handle_abierto_falla_cerrado() {
    let fixture = fixture();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let path = fixture.parent.join("candidate.journal");
    fs::write(&path, b"scope canary").unwrap();
    let scope_real = owner.scope_parent;
    let scope_distinto = super::ScopeFilesystem(scope_real.0.wrapping_add(1));

    let resultado =
        observar_sibling_con_scope(&owner.parent, path.file_name().unwrap(), scope_real, |_| {
            Ok(scope_distinto)
        });

    assert!(matches!(
        resultado,
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));
    assert_eq!(fs::read(path).unwrap(), b"scope canary");
}

#[test]
fn error_al_obtener_scope_falla_cerrado_y_preserva_el_archivo() {
    let fixture = fixture();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let path = fixture.parent.join("candidate.journal");
    fs::write(&path, b"scope error canary").unwrap();

    let resultado = observar_sibling_con_scope(
        &owner.parent,
        path.file_name().unwrap(),
        owner.scope_parent,
        |_| Err(crate::transaccion::anclado::ErrorDirectorioAnclado::Io),
    );

    assert!(matches!(
        resultado,
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));
    assert_eq!(fs::read(path).unwrap(), b"scope error canary");
}
