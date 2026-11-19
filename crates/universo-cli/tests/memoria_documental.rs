//! La plantilla de memoria documental (docs/07) se admite y sus reglas con
//! nombre explican una violación.
use std::{fs, path::Path, process::Command};

fn copiar(origen: &Path, destino: &Path) {
    fs::create_dir_all(destino).unwrap();
    for entrada in fs::read_dir(origen).unwrap() {
        let entrada = entrada.unwrap();
        let ruta = entrada.path();
        if ruta.is_dir() {
            copiar(&ruta, &destino.join(entrada.file_name()));
        } else {
            fs::copy(&ruta, destino.join(entrada.file_name())).unwrap();
        }
    }
}

fn check(dir: &Path, extra: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .arg("check")
        .arg(dir.join("bundle"))
        .arg("--manifiesto")
        .arg(dir.join("manifest.toml"))
        .arg("--invariantes")
        .arg(dir.join("invariants.sx"))
        .args(extra)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
    )
}

#[test]
fn plantilla_se_admite_y_un_criterio_firme_sin_resolucion_se_rechaza_con_nombre() {
    let ejemplo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/memoria-documental");
    let (code, out) = check(&ejemplo, &[]);
    assert_eq!(code, 0, "{out}");
    let tmp = tempfile::tempdir().unwrap();
    copiar(&ejemplo, tmp.path());
    let resolucion = tmp.path().join("bundle/notas/resolucion-cache.md");
    let texto = fs::read_to_string(&resolucion).unwrap();
    // Por líneas: en Windows el checkout puede traer CRLF.
    let sin_justificacion: String = texto
        .split_inclusive('\n')
        .filter(|linea| !linea.starts_with("justifica:"))
        .collect();
    assert_ne!(sin_justificacion, texto, "la línea justifica debe existir");
    fs::write(&resolucion, sin_justificacion).unwrap();
    let (code, out) = check(tmp.path(), &["--explain", "decide"]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains(
            "rule: id=ID1 message=Todo criterio firme tiene una resolución que lo justifica."
        ),
        "{out}"
    );
}
