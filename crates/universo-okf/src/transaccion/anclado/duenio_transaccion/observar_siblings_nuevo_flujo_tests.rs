use super::*;
use crate::transaccion::anclado::contexto_admision::{
    nombre_canonico as nombre_contexto_canonico,
    nombre_temporal_sibling as nombre_contexto_temporal,
};
use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use std::ffi::OsString;
use std::fs;
use std::fs::File;
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

struct FixtureCuatro {
    _temp: TempDir,
    parent: PathBuf,
    root: PathBuf,
    ubicacion: UbicacionRaiz,
    nombres: [OsString; 4],
}

fn fixture_cuatro(root_presente: bool) -> FixtureCuatro {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("parent");
    fs::create_dir(&parent).unwrap();
    let root = parent.join("bundle");
    if root_presente {
        fs::create_dir(&root).unwrap();
    }
    let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
    fs::write(ubicacion.lock().as_path(), LOCK_BYTES).unwrap();

    let ruta_journal = ubicacion.journal();
    let nombre_journal = ruta_journal.as_path().file_name().unwrap().to_os_string();
    let nombre_journal_temporal = ubicacion.nombre_journal_temporal();
    let basename = ubicacion.ruta().file_name().unwrap();
    let nombre_contexto = nombre_contexto_canonico(basename).unwrap();
    let nombre_contexto_temporal = nombre_contexto_temporal(&nombre_contexto).unwrap();

    FixtureCuatro {
        _temp: temp,
        parent,
        root,
        ubicacion,
        nombres: [
            nombre_journal,
            nombre_journal_temporal,
            nombre_contexto,
            nombre_contexto_temporal,
        ],
    }
}

fn presencias(observacion: ObservacionSiblingsNuevoFlujo) -> [PresenciaSiblingNuevoFlujo; 4] {
    [
        observacion.journal(),
        observacion.journal_temporal(),
        observacion.contexto(),
        observacion.contexto_temporal(),
    ]
}

const DECOY_SIBLINGS: [&[u8]; 4] = [
    b"decoy journal",
    b"decoy journal temporary",
    b"decoy context",
    b"decoy context temporary",
];

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

#[test]
fn owner_reporta_los_cuatro_ausentes_con_root_presente_o_ausente() {
    for root_presente in [false, true] {
        let fixture = fixture_cuatro(root_presente);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let observado = owner.observar_siblings_nuevo_flujo().unwrap();

        assert_eq!(
            presencias(observado),
            [PresenciaSiblingNuevoFlujo::Ausente; 4]
        );
        assert_eq!(fixture.root.is_dir(), root_presente);
    }
}

#[test]
fn owner_reporta_cada_sibling_regular_por_separado() {
    for presente in 0..4 {
        let fixture = fixture_cuatro(presente % 2 == 0);
        let path = fixture.parent.join(&fixture.nombres[presente]);
        fs::write(&path, b"regular sibling canary").unwrap();
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let observado = presencias(owner.observar_siblings_nuevo_flujo().unwrap());
        let mut esperado = [PresenciaSiblingNuevoFlujo::Ausente; 4];
        esperado[presente] = PresenciaSiblingNuevoFlujo::Presente;

        assert_eq!(observado, esperado);
        assert_eq!(fs::read(path).unwrap(), b"regular sibling canary");
    }
}

#[test]
fn owner_reporta_los_cuatro_siblings_presentes() {
    let fixture = fixture_cuatro(false);
    for nombre in &fixture.nombres {
        fs::write(fixture.parent.join(nombre), b"present sibling").unwrap();
    }
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let observado = owner.observar_siblings_nuevo_flujo().unwrap();

    assert_eq!(
        presencias(observado),
        [PresenciaSiblingNuevoFlujo::Presente; 4]
    );
}

#[test]
fn parent_movido_con_lock_y_siblings_decoy_sigue_el_handle_original() {
    let fixture = fixture_cuatro(false);
    let moved_parent = fixture.parent.with_file_name("parent-moved");
    let sibling_original = fixture.parent.join(&fixture.nombres[0]);
    #[cfg(windows)]
    let lock_path = fixture.ubicacion.lock().as_path().to_path_buf();
    fs::write(&sibling_original, b"original journal sibling").unwrap();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let identidad_guard = lock.identidad_archivo_lock().unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    if let Err(error) = fs::rename(&fixture.parent, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(matches!(
                error.raw_os_error(),
                Some(5) | Some(32) | Some(33)
            ));
            assert!(fixture.parent.is_dir());
            assert!(!moved_parent.exists());
            assert_eq!(
                owner._lock.identidad_archivo_lock().unwrap(),
                identidad_guard
            );
            assert_eq!(
                presencias(owner.observar_siblings_nuevo_flujo().unwrap()),
                [
                    PresenciaSiblingNuevoFlujo::Presente,
                    PresenciaSiblingNuevoFlujo::Ausente,
                    PresenciaSiblingNuevoFlujo::Ausente,
                    PresenciaSiblingNuevoFlujo::Ausente,
                ]
            );
            drop(owner);
            drop(lock);
            assert_eq!(
                fs::read(&sibling_original).unwrap(),
                b"original journal sibling"
            );
            assert_eq!(fs::read(lock_path).unwrap(), LOCK_BYTES);
            eprintln!(
                "Windows impidió mover el parent retenido; el namespace decoy no se ejercitó."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent anclado de prueba: {error}");
    }

    fs::create_dir(&fixture.parent).unwrap();
    let ruta_lock = fixture.ubicacion.lock();
    let lock_decoy = fixture
        .parent
        .join(ruta_lock.as_path().file_name().unwrap());
    fs::write(&lock_decoy, LOCK_BYTES).unwrap();
    let mut rutas_decoy = Vec::new();
    for (nombre, bytes) in fixture.nombres.iter().zip(DECOY_SIBLINGS) {
        let path = fixture.parent.join(nombre);
        fs::write(&path, bytes).unwrap();
        rutas_decoy.push(path);
    }
    let decoy_handle = File::open(&lock_decoy).unwrap();
    assert_ne!(
        crate::transaccion::anclado::identidad_archivo(&decoy_handle).unwrap(),
        identidad_guard
    );

    let observado = owner.observar_siblings_nuevo_flujo().unwrap();

    assert_eq!(
        presencias(observado),
        [
            PresenciaSiblingNuevoFlujo::Presente,
            PresenciaSiblingNuevoFlujo::Ausente,
            PresenciaSiblingNuevoFlujo::Ausente,
            PresenciaSiblingNuevoFlujo::Ausente,
        ]
    );
    assert_eq!(
        fs::read(moved_parent.join(&fixture.nombres[0])).unwrap(),
        b"original journal sibling"
    );
    assert!(
        fixture.nombres[1..]
            .iter()
            .all(|nombre| !moved_parent.join(nombre).exists())
    );
    assert!(
        rutas_decoy
            .iter()
            .zip(DECOY_SIBLINGS)
            .all(|(path, bytes)| fs::read(path).unwrap().as_slice() == bytes)
    );
    assert!(!moved_parent.join("bundle").exists());
    assert!(!fixture.root.exists());

    drop(owner);
    drop(lock);
    assert_eq!(
        fs::read(moved_parent.join(lock_decoy.file_name().unwrap())).unwrap(),
        LOCK_BYTES
    );
    assert_eq!(fs::read(lock_decoy).unwrap(), LOCK_BYTES);
}
