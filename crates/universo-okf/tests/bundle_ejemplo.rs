//! Integración contra el bundle público y ficticio incluido en `examples/`.

use universo_core::BundlePath;
use universo_okf::{cargar_bundle, cargar_manifiesto, serializar_bundle};
use universo_testkit::ejemplo;

#[test]
fn el_bundle_publico_carga_sin_errores() {
    let manifiesto = cargar_manifiesto(&ejemplo::manifiesto()).unwrap();
    let universo = cargar_bundle(&ejemplo::bundle(), manifiesto).unwrap_or_else(|errores| {
        panic!(
            "{} errores:\n{}",
            errores.len(),
            errores
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    });

    assert_eq!(universo.len_nodos(), 3);
    assert_eq!(universo.len_aristas(), 1);
    let atomos: Vec<String> = universo.atomos().iter().map(ToString::to_string).collect();
    assert!(atomos.contains(&"Parent(knowledge/cache-policy, knowledge/index)".to_string()));
    assert!(atomos.contains(&"Edge(knowledge/cache-policy→knowledge/reliability#supports#0, knowledge/cache-policy, knowledge/reliability, supports)".to_string()));
}

#[test]
fn el_ejemplo_conserva_semantica_en_round_trip() {
    let manifiesto = cargar_manifiesto(&ejemplo::manifiesto()).unwrap();
    let original = cargar_bundle(&ejemplo::bundle(), manifiesto.clone()).unwrap();
    let salida = tempfile::tempdir().unwrap();
    serializar_bundle(&original, salida.path()).unwrap();
    let recargado = cargar_bundle(salida.path(), manifiesto).unwrap();

    assert_eq!(original.atomos(), recargado.atomos());
    let ruta = BundlePath::nuevo("knowledge/cache-policy").unwrap();
    let original_id = original.por_path(&ruta).unwrap();
    let recargado_id = recargado.por_path(&ruta).unwrap();
    assert_eq!(original.anexo(original_id), recargado.anexo(recargado_id));
}
