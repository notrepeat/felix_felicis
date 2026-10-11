use std::collections::BTreeSet;

use universo_core::{Atomo, BundlePath, DeclPredicado, Delta, Sorte};
use universo_okf::{
    parsear_bundle, parsear_delta_json_v1, serializar_delta_json_v1, serializar_nodo,
};

#[test]
fn json_ext_canonico_se_parsea_y_serializa_sin_perdida() {
    let json = r#"{"schema":"felix-delta-v1","add":["Nivel(n/root, bajo)"],"remove":["Nivel(n/root, alto)"]}"#;

    let delta = parsear_delta_json_v1(json).unwrap();
    let serializado = serializar_delta_json_v1(&delta).unwrap();

    assert_eq!(serializado, json);
    assert_eq!(parsear_delta_json_v1(&serializado).unwrap(), delta);
    assert!(
        matches!(delta.agregar(), [Atomo::Ext { predicado, sujeto, constante }]
        if predicado == "Nivel" && sujeto.como_str() == "n/root" && constante == "bajo")
    );
    assert!(
        matches!(delta.retirar(), [Atomo::Ext { predicado, sujeto, constante }]
        if predicado == "Nivel" && sujeto.como_str() == "n/root" && constante == "alto")
    );
}

#[test]
fn candidato_ext_elimina_y_reemplaza_el_campo_al_serializar_y_recargar() {
    let manifiesto = universo_testkit::manifiesto_minimo();
    let archivo = (
        "n/root.md".to_string(),
        "---\ntype: a\ntitle: Raíz\nnivel: alto\n---\ncuerpo\n".to_string(),
    );
    let snapshot = parsear_bundle(&[archivo], manifiesto.clone()).unwrap();
    let sujeto = BundlePath::nuevo("n/root").unwrap();
    let id = snapshot.por_path(&sujeto).unwrap();

    let retiro = Delta::nuevo(
        [],
        [Atomo::Ext {
            predicado: "Nivel".into(),
            sujeto: sujeto.clone(),
            constante: "alto".into(),
        }],
    )
    .unwrap();
    let candidato_retiro = universo_core::construir_candidato(&snapshot, &retiro).unwrap();
    let serializado_retiro = serializar_nodo(candidato_retiro.estructura(), id);
    assert!(!serializado_retiro.contains("nivel:"));
    let recargado_retiro = parsear_bundle(
        &[("n/root.md".to_string(), serializado_retiro)],
        manifiesto.clone(),
    )
    .unwrap();
    assert!(
        !recargado_retiro
            .atomos()
            .iter()
            .any(|atomo| matches!(atomo, Atomo::Ext { predicado, .. } if predicado == "Nivel"))
    );

    let reemplazo = Delta::nuevo(
        [Atomo::Ext {
            predicado: "Nivel".into(),
            sujeto,
            constante: "bajo".into(),
        }],
        [Atomo::Ext {
            predicado: "Nivel".into(),
            sujeto: BundlePath::nuevo("n/root").unwrap(),
            constante: "alto".into(),
        }],
    )
    .unwrap();
    let candidato_reemplazo = universo_core::construir_candidato(&snapshot, &reemplazo).unwrap();
    let serializado_reemplazo = serializar_nodo(candidato_reemplazo.estructura(), id);
    assert!(serializado_reemplazo.contains("nivel: bajo\n"));
    assert!(!serializado_reemplazo.contains("nivel: alto\n"));
    let recargado_reemplazo = parsear_bundle(
        &[("n/root.md".to_string(), serializado_reemplazo)],
        manifiesto,
    )
    .unwrap();
    assert!(recargado_reemplazo.atomos().iter().any(
        |atomo| matches!(atomo, Atomo::Ext { predicado, sujeto, constante }
            if predicado == "Nivel" && sujeto.como_str() == "n/root" && constante == "bajo")
    ));
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
