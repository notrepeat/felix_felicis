use std::collections::BTreeSet;

use universo_core::{Atomo, BundlePath, DeclPredicado, Delta, Sorte};
use universo_okf::{
    parsear_bundle, parsear_delta_json_v1, serializar_delta_json_v1, serializar_nodo,
};

#[test]
fn json_ext_canonico_se_parsea_y_serializa_sin_perdida() {
    let json = r#"{"schema":"felix-delta-v1","add":["Nivel(n/root, alto)"],"remove":[]}"#;

    let delta = parsear_delta_json_v1(json).unwrap();
    let serializado = serializar_delta_json_v1(&delta).unwrap();

    assert_eq!(serializado, json);
    assert_eq!(parsear_delta_json_v1(&serializado).unwrap(), delta);
    assert!(
        matches!(delta.agregar(), [Atomo::Ext { predicado, sujeto, constante }]
        if predicado == "Nivel" && sujeto.como_str() == "n/root" && constante == "alto")
    );
}

#[test]
fn candidato_ext_recarga_con_yaml_entrecomillado_y_passthrough_crudo() {
    let mut manifiesto = universo_testkit::manifiesto_minimo();
    manifiesto.predicados.push(DeclPredicado {
        nombre: "Modo".into(),
        clave: "modo".into(),
        dominio: BTreeSet::from(["-alto".to_string()]),
    });
    let archivo = (
        "n/root.md".to_string(),
        "---\ntype: a\ntitle: Raíz\nextra: \"valor: con # y \\\"comillas\\\" \\\\ barra\"\n---\ncuerpo # preservado\n".to_string(),
    );
    let snapshot = parsear_bundle(&[archivo], manifiesto.clone()).unwrap();
    let id = snapshot
        .por_path(&BundlePath::nuevo("n/root").unwrap())
        .unwrap();
    let delta = Delta::nuevo(
        [
            Atomo::Ext {
                predicado: "Nivel".into(),
                sujeto: BundlePath::nuevo("n/root").unwrap(),
                constante: "alto".into(),
            },
            Atomo::Ext {
                predicado: "Modo".into(),
                sujeto: BundlePath::nuevo("n/root").unwrap(),
                constante: "-alto".into(),
            },
        ],
        [],
    )
    .unwrap();
    let candidato = universo_core::construir_candidato(&snapshot, &delta).unwrap();
    let serializado = serializar_nodo(candidato.estructura(), id);

    assert!(serializado.contains("modo: \"-alto\"\n"));
    assert!(serializado.contains("extra: \"valor: con # y \\\"comillas\\\" \\\\ barra\"\n"));
    assert!(serializado.ends_with("cuerpo # preservado\n"));

    let recargado = parsear_bundle(&[("n/root.md".to_string(), serializado)], manifiesto).unwrap();
    let recargado_id = recargado
        .por_path(&BundlePath::nuevo("n/root").unwrap())
        .unwrap();
    assert_eq!(recargado.atomos(), candidato.estructura().atomos());
    let anexo = recargado.anexo(recargado_id).unwrap();
    assert_eq!(anexo.cuerpo, "cuerpo # preservado\n");
    assert_eq!(anexo.passthrough.len(), 1);
    assert_eq!(
        anexo.passthrough[0].crudo,
        "extra: \"valor: con # y \\\"comillas\\\" \\\\ barra\"\n"
    );
    assert_eq!(
        recargado.nodo(recargado_id).unwrap().tipo,
        Sorte::nuevo("a")
    );
}
