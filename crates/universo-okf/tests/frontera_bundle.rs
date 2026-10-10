//! El cargador no debe ingerir datos fuera de la raíz del bundle.

use std::path::{Path, PathBuf};

use universo_okf::{ErrorBundle, Regla, cargar_bundle};
use universo_testkit::manifiesto_minimo;

struct EspacioPrueba {
    _temporal: tempfile::TempDir,
    raiz: PathBuf,
    fuera: PathBuf,
}

fn espacio_prueba() -> EspacioPrueba {
    let temporal = tempfile::tempdir().unwrap();
    let raiz = temporal.path().join("bundle");
    let fuera = temporal.path().join("fuera");
    std::fs::create_dir_all(&raiz).unwrap();
    std::fs::create_dir_all(&fuera).unwrap();
    EspacioPrueba {
        _temporal: temporal,
        raiz,
        fuera,
    }
}

fn documento_externo_ilegal(dir: &Path) {
    std::fs::write(dir.join("secreto.md"), b"\xffcontenido fuera del bundle").unwrap();
}

fn manifiesto_con_directorios(directorios: Vec<String>) -> universo_core::Manifiesto {
    let mut manifiesto = manifiesto_minimo();
    manifiesto.layout.directorios = directorios;
    manifiesto
}

fn exigir_rechazo_sin_leer_archivo_externo(
    resultado: Result<universo_core::Estructura, Vec<ErrorBundle>>,
) {
    let errores = resultado.expect_err("una ruta fuera del bundle no debe cargarse");
    assert!(
        errores.iter().any(|error| error.regla == Regla::Io),
        "se esperaba un error de frontera: {errores:?}"
    );
    assert!(
        !errores.iter().any(|error| error.regla == Regla::NoUtf8),
        "el contenido externo llegó al lector de documentos: {errores:?}"
    );
}

#[test]
fn rechaza_directorio_absoluto_de_layout_antes_de_leerlo() {
    let espacio = espacio_prueba();
    documento_externo_ilegal(&espacio.fuera);
    let directorio_absoluto = espacio.fuera.to_string_lossy().into_owned();

    let resultado = cargar_bundle(
        &espacio.raiz,
        manifiesto_con_directorios(vec![directorio_absoluto]),
    );

    exigir_rechazo_sin_leer_archivo_externo(resultado);
}

#[test]
fn rechaza_parent_que_escapa_de_la_raiz_antes_de_leerlo() {
    let espacio = espacio_prueba();
    documento_externo_ilegal(&espacio.fuera);
    // El contrato del manifiesto usa `/` en todas las plataformas.
    let ruta_relativa = "../fuera".to_string();

    let resultado = cargar_bundle(
        &espacio.raiz,
        manifiesto_con_directorios(vec![ruta_relativa]),
    );

    exigir_rechazo_sin_leer_archivo_externo(resultado);
}

#[cfg(unix)]
fn crear_symlink_directorio(destino: &Path, enlace: &Path) {
    std::os::unix::fs::symlink(destino, enlace).unwrap();
}

#[cfg(windows)]
fn crear_symlink_directorio(destino: &Path, enlace: &Path) {
    if let Err(error) = std::os::windows::fs::symlink_dir(destino, enlace) {
        eprintln!("aviso: symlink de directorio no disponible ({error}); se usa una junction");
        let resultado = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(enlace)
            .arg(destino)
            .output()
            .expect("cmd debe poder ejecutarse para crear una junction");
        assert!(
            resultado.status.success(),
            "no se pudo crear symlink ni junction: {}",
            String::from_utf8_lossy(&resultado.stderr)
        );
    }
}

#[test]
fn rechaza_directorio_symlink_hacia_fuera_antes_de_leerlo() {
    let espacio = espacio_prueba();
    documento_externo_ilegal(&espacio.fuera);
    crear_symlink_directorio(&espacio.fuera, &espacio.raiz.join("n"));

    let resultado = cargar_bundle(&espacio.raiz, manifiesto_con_directorios(vec!["n".into()]));

    exigir_rechazo_sin_leer_archivo_externo(resultado);
}

#[cfg(unix)]
fn crear_symlink_archivo(destino: &Path, enlace: &Path) -> bool {
    std::os::unix::fs::symlink(destino, enlace).unwrap();
    true
}

#[cfg(windows)]
fn crear_symlink_archivo(destino: &Path, enlace: &Path) -> bool {
    match std::os::windows::fs::symlink_file(destino, enlace) {
        Ok(()) => true,
        Err(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(1314) =>
        {
            eprintln!("aviso: symlink de archivo omitido por falta de privilegio ({error})");
            false
        }
        Err(error) => panic!("no se pudo crear symlink de archivo: {error}"),
    }
}

#[test]
fn rechaza_symlink_de_archivo_hacia_fuera_antes_de_leerlo() {
    let espacio = espacio_prueba();
    documento_externo_ilegal(&espacio.fuera);
    std::fs::create_dir(espacio.raiz.join("n")).unwrap();
    let enlace = espacio.raiz.join("n/secreto.md");
    if !crear_symlink_archivo(&espacio.fuera.join("secreto.md"), &enlace) {
        return;
    }

    let resultado = cargar_bundle(&espacio.raiz, manifiesto_con_directorios(vec!["n".into()]));

    exigir_rechazo_sin_leer_archivo_externo(resultado);
}
