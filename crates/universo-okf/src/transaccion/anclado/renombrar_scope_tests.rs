use super::renombrar::ErrorRenombrarAnclado;
use super::renombrar_test_support::{rename_backup_name, rename_identity, saltar_sin_file_id};
use super::{DirectorioAnclado, ErrorDirectorioAnclado, UbicacionRaiz};
use std::cell::Cell;
use std::ffi::OsStr;
use std::fs::File;
use tempfile::tempdir;

#[test]
fn rename_sibling_scope_pre_y_post_falla_cerrado() {
    use crate::transaccion::lock::LockEscritura;

    for llamada_fallida in [2, 5] {
        for fallo_error in [true, false] {
            let temp = tempdir().unwrap();
            if saltar_sin_file_id(temp.path()) {
                return;
            }
            let root = temp.path().join("bundle");
            std::fs::create_dir(&root).unwrap();
            let mut lock = LockEscritura::adquirir(&root).unwrap();
            let mut espacio = lock.espacio_transaccional_anclado().unwrap();
            let identidad = rename_identity(&root);
            let mut source = espacio
                .observar_directorio_sibling(OsStr::new("bundle"), identidad)
                .unwrap();
            let destino = rename_backup_name(&root);
            let llamadas_probe = Cell::new(0);
            let mut probe = |_handle: &File| {
                let llamada = llamadas_probe.get() + 1;
                llamadas_probe.set(llamada);
                if llamada == llamada_fallida && fallo_error {
                    return Err(ErrorDirectorioAnclado::Io);
                }
                if llamada == llamada_fallida {
                    Ok(false)
                } else {
                    Ok(true)
                }
            };
            let llamadas_rename = Cell::new(0);
            let parent = temp.path().to_path_buf();
            let mut rename = |_: &File, desde: &OsStr, hacia: &OsStr, _: &File, _| {
                llamadas_rename.set(llamadas_rename.get() + 1);
                std::fs::rename(parent.join(desde), parent.join(hacia))
                    .map_err(|_| ErrorRenombrarAnclado::Io)
            };

            let error = espacio
                .renombrar_directorio_con_probe_y_operacion(
                    &mut source,
                    &destino,
                    &mut probe,
                    &mut rename,
                )
                .unwrap_err();

            if llamada_fallida == 2 {
                assert_eq!(error, ErrorRenombrarAnclado::ScopeNoVerificado);
                assert_eq!(llamadas_rename.get(), 0);
                assert_eq!(source.nombre(), OsStr::new("bundle"));
                assert!(root.is_dir());
                assert!(!parent.join(&destino).exists());
            } else {
                assert_eq!(error, ErrorRenombrarAnclado::ResultadoNoVerificado);
                assert_eq!(llamadas_rename.get(), 1);
                assert_eq!(source.nombre(), OsStr::new("bundle"));
                assert!(!root.exists());
                assert!(parent.join(&destino).is_dir());
                assert_eq!(
                    espacio.renombrar_directorio_con_probe_y_operacion(
                        &mut source,
                        OsStr::new("retry"),
                        &mut |_| Ok(true),
                        &mut rename,
                    ),
                    Err(ErrorRenombrarAnclado::ResultadoNoVerificado)
                );
                assert_eq!(llamadas_rename.get(), 1);
            }
        }
    }
}

#[test]
fn rename_sibling_syscall_error_conserva_codigo_pero_invalida_capability() {
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = temp.path().join("bundle");
    std::fs::create_dir(&root).unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut espacio = lock.espacio_transaccional_anclado().unwrap();
    let identidad = rename_identity(&root);
    let mut source = espacio
        .observar_directorio_sibling(OsStr::new("bundle"), identidad)
        .unwrap();
    let destino = rename_backup_name(&root);
    let mut probe = |_handle: &File| Ok(true);
    let llamadas = Cell::new(0);
    let mut rename = |_: &File, _: &OsStr, _: &OsStr, _: &File, _| {
        llamadas.set(llamadas.get() + 1);
        Err(ErrorRenombrarAnclado::Io)
    };

    assert_eq!(
        espacio.renombrar_directorio_con_probe_y_operacion(
            &mut source,
            &destino,
            &mut probe,
            &mut rename,
        ),
        Err(ErrorRenombrarAnclado::Io)
    );
    assert_eq!(source.nombre(), OsStr::new("bundle"));
    assert!(root.is_dir());
    assert!(!temp.path().join(&destino).exists());
    assert_eq!(
        espacio.renombrar_directorio_con_probe_y_operacion(
            &mut source,
            &destino,
            &mut probe,
            &mut rename,
        ),
        Err(ErrorRenombrarAnclado::ResultadoNoVerificado)
    );
    assert_eq!(llamadas.get(), 1);
}

#[test]
fn rename_sibling_rechaza_capability_de_otro_lock_en_el_mismo_parent() {
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root_a = temp.path().join("bundle-a");
    let root_b = temp.path().join("bundle-b");
    std::fs::create_dir(&root_a).unwrap();
    std::fs::create_dir(&root_b).unwrap();
    std::fs::write(root_a.join("canary.txt"), b"root a").unwrap();
    std::fs::write(root_b.join("canary.txt"), b"root b").unwrap();
    let mut lock_a = LockEscritura::adquirir(&root_a).unwrap();
    let mut lock_b = LockEscritura::adquirir(&root_b).unwrap();
    let mut espacio_a = lock_a.espacio_transaccional_anclado().unwrap();
    let espacio_b = lock_b.espacio_transaccional_anclado().unwrap();
    let identidad_b = rename_identity(&root_b);
    let mut sibling_b = espacio_b
        .observar_directorio_sibling(std::ffi::OsStr::new("bundle-b"), identidad_b)
        .unwrap();

    let backup_a = rename_backup_name(&root_a);
    let error = espacio_a
        .renombrar_directorio(&mut sibling_b, &backup_a)
        .unwrap_err();

    assert_eq!(error, ErrorRenombrarAnclado::FuenteNoHermana);
    assert_eq!(sibling_b.identidad(), identidad_b);
    assert_eq!(sibling_b.nombre(), std::ffi::OsStr::new("bundle-b"));
    assert!(matches!(
        espacio_a.observar_directorio_sibling(std::ffi::OsStr::new("bundle-b"), identidad_b),
        Err(ErrorRenombrarAnclado::FuenteNoHermana)
    ));
    let identity_a = rename_identity(&root_a);
    let mut sibling_a = espacio_a
        .observar_directorio_sibling(std::ffi::OsStr::new("bundle-a"), identity_a)
        .unwrap();
    assert!(matches!(
        espacio_a.renombrar_directorio(&mut sibling_a, std::ffi::OsStr::new("bundle-b")),
        Err(ErrorRenombrarAnclado::FuenteNoHermana)
    ));
    assert_eq!(sibling_a.identidad(), identity_a);
    assert_eq!(sibling_a.nombre(), std::ffi::OsStr::new("bundle-a"));
    assert_eq!(std::fs::read(root_a.join("canary.txt")).unwrap(), b"root a");
    assert_eq!(std::fs::read(root_b.join("canary.txt")).unwrap(), b"root b");
    assert!(!temp.path().join(&backup_a).exists());
}

#[test]
fn rename_sibling_rechaza_identidad_fuente_reemplazada() {
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = temp.path().join("bundle");
    std::fs::create_dir(&root).unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut espacio = lock.espacio_transaccional_anclado().unwrap();
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&root).unwrap()).unwrap();
    let (stage_anchor, component) = ancla.crear_stage_sibling("identity-mismatch").unwrap();
    let source_path = temp.path().join(component);
    let saved_path = temp.path().join("saved-source");
    std::fs::write(source_path.join("original.txt"), b"original source").unwrap();
    let original_identity = stage_anchor.identidad_raiz().unwrap();
    let mut source = espacio
        .observar_directorio_sibling(source_path.file_name().unwrap(), original_identity)
        .unwrap();
    std::fs::rename(&source_path, &saved_path).unwrap();
    std::fs::create_dir(&source_path).unwrap();
    std::fs::write(source_path.join("canary.txt"), b"replacement").unwrap();
    let replacement =
        DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&source_path).unwrap()).unwrap();

    let backup_name = rename_backup_name(&root);
    let error = espacio
        .renombrar_directorio(&mut source, &backup_name)
        .unwrap_err();

    assert_eq!(error, ErrorRenombrarAnclado::IdentidadNoCoincide);
    assert_ne!(replacement.identidad_raiz(), Some(original_identity));
    assert_eq!(source.nombre(), source_path.file_name().unwrap());
    assert_eq!(source.identidad(), original_identity);
    assert_eq!(
        std::fs::read(source_path.join("canary.txt")).unwrap(),
        b"replacement"
    );
    assert!(saved_path.is_dir());
    assert_eq!(
        std::fs::read(saved_path.join("original.txt")).unwrap(),
        b"original source"
    );
    assert!(!temp.path().join(&backup_name).exists());
}

#[test]
fn rename_sibling_rechaza_nombres_no_componentes() {
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = temp.path().join("bundle");
    std::fs::create_dir(&root).unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut espacio = lock.espacio_transaccional_anclado().unwrap();
    let mut source = espacio
        .observar_directorio_sibling(std::ffi::OsStr::new("bundle"), rename_identity(&root))
        .unwrap();

    let invalid_names = vec![
        "",
        ".",
        "..",
        "a/b",
        "a\\b",
        "file:stream",
        "CON",
        "nul.txt",
        "\0",
        "\u{1}",
    ];
    #[cfg(windows)]
    let invalid_names = {
        let mut invalid_names = invalid_names;
        invalid_names.extend(["normal.", "normal ", "BUNDLE~23"]);
        invalid_names
    };
    for nombre in invalid_names {
        let error = espacio
            .renombrar_directorio(&mut source, std::ffi::OsStr::new(nombre))
            .unwrap_err();
        assert_eq!(error, ErrorRenombrarAnclado::ComponenteInvalido, "{nombre}");
    }
    assert!(root.is_dir());
}

#[test]
fn rename_sibling_rechaza_hermano_que_no_es_directorio() {
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    let backup_name = rename_backup_name(&root);
    std::fs::create_dir(&root).unwrap();
    std::fs::write(temp.path().join(&backup_name), b"file canary").unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let espacio = lock.espacio_transaccional_anclado().unwrap();

    assert!(matches!(
        espacio.observar_directorio_sibling(&backup_name, rename_identity(&root)),
        Err(ErrorRenombrarAnclado::TipoNoAdmitido)
    ));
    assert_eq!(
        std::fs::read(temp.path().join(&backup_name)).unwrap(),
        b"file canary"
    );
    assert!(root.is_dir());
}
