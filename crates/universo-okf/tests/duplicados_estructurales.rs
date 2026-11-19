//! Las claves estructurales singulares no deben resolverse usando el primer valor.

use universo_okf::{ErrorBundle, Regla, cargar_bundle};
use universo_testkit::manifiesto_minimo;

fn errores_para_documento(texto: &str) -> Vec<ErrorBundle> {
    let temporal = tempfile::tempdir().unwrap();
    let directorio = temporal.path().join("n");
    std::fs::create_dir_all(&directorio).unwrap();
    std::fs::write(directorio.join("x.md"), texto).unwrap();

    cargar_bundle(temporal.path(), manifiesto_minimo()).unwrap_err()
}

fn exigir_error_duplicado(errores: &[ErrorBundle], linea: usize, clave: &str) {
    assert!(
        errores.iter().any(|error| {
            error.path.as_deref() == Some("n/x")
                && error.linea == Some(linea)
                && error.regla == Regla::ClaveEstructuralDuplicada
                && error.detalle.contains(clave)
                && error.detalle.contains("duplicad")
        }),
        "se esperaba un diagnóstico explícito para `{clave}`: {errores:?}"
    );
}

#[test]
fn type_y_title_duplicados_fallan_con_diagnostico_explicito() {
    for (texto, clave) in [
        ("---\ntype: a\ntitle: X\ntype: b\n---\n", "type"),
        ("---\ntype: a\ntitle: X\ntitle: Y\n---\n", "title"),
    ] {
        let errores = errores_para_documento(texto);
        exigir_error_duplicado(&errores, 4, clave);
    }
}

#[test]
fn destino_y_etiqueta_duplicados_fallan_en_mappings() {
    for (texto, clave) in [
        (
            "---\ntype: a\ntitle: X\nten:\n  - with: [[x]]\n    with: [[y]]\n    scope: S\n---\n",
            "with",
        ),
        (
            "---\ntype: a\ntitle: X\nten:\n  - with: [[x]]\n    scope: S\n    scope: T\n---\n",
            "scope",
        ),
    ] {
        let errores = errores_para_documento(texto);
        exigir_error_duplicado(&errores, 4, clave);
    }
}
