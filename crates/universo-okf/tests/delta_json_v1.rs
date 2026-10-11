use universo_core::{Atomo, BundlePath, Delta, Etiqueta, Sorte, Sujeto};
use universo_okf::{ErrorDeltaJson, LadoDelta, parsear_delta_json_v1, serializar_delta_json_v1};

fn json(add: &str, remove: &str) -> String {
    format!(r#"{{"schema":"felix-delta-v1","add":{add},"remove":{remove}}}"#)
}

fn type_atom(path: String) -> Atomo {
    Atomo::Type {
        sujeto: BundlePath::nuevo(&path).unwrap(),
        sorte: Sorte::nuevo("a"),
    }
}

#[test]
fn parsea_json_v1_y_rechaza_schema_claves_y_claves_duplicadas() {
    assert!(matches!(
        parsear_delta_json_v1(r#"{"schema":"felix-delta-v2","add":[],"remove":[]}"#),
        Err(ErrorDeltaJson::SchemaNoSoportado)
    ));
    let error =
        parsear_delta_json_v1(r#"{"schema":"PRIVATE_SCHEMA","add":[],"remove":[]}"#).unwrap_err();
    assert!(!format!("{error} {error:?}").contains("PRIVATE_SCHEMA"));
    assert!(matches!(
        parsear_delta_json_v1(r#"{"schema":"felix-delta-v1","add":[],"remove":[],"extra":1}"#),
        Err(ErrorDeltaJson::Json)
    ));
    assert!(matches!(
        parsear_delta_json_v1(r#"{"schema":"felix-delta-v1","add":[],"add":[],"remove":[]}"#),
        Err(ErrorDeltaJson::Json)
    ));
    assert!(matches!(
        parsear_delta_json_v1(r#"{"schema":"felix-delta-v1","add":[]}"#),
        Err(ErrorDeltaJson::Json)
    ));
    assert!(matches!(
        parsear_delta_json_v1(r#"{"schema":"felix-delta-v1","add":"Type(n/a, a)","remove":[]}"#),
        Err(ErrorDeltaJson::Json)
    ));
    let error = parsear_delta_json_v1(
        r#"{"schema":"felix-delta-v1","add":[],"remove":[],"PRIVATE_KEY":1}"#,
    )
    .unwrap_err();
    assert!(!format!("{error} {error:?}").contains("PRIVATE_KEY"));
}

#[test]
fn rechaza_atomos_invalidos_no_canonicos_duplicados_y_solapados() {
    assert!(matches!(
        parsear_delta_json_v1(&json(r#"["Type(n/a, a)","Type(n/a, a)"]"#, "[]")),
        Err(ErrorDeltaJson::AtomoDuplicado {
            lado: LadoDelta::Add,
            indice: 1
        })
    ));
    assert!(matches!(
        parsear_delta_json_v1(&json(r#"["Edge(n/a→n/b#rel#00, n/a, n/b, rel)"]"#, "[]")),
        Err(ErrorDeltaJson::AtomoNoCanonico {
            lado: LadoDelta::Add,
            indice: 0
        })
    ));
    assert!(matches!(
        parsear_delta_json_v1(&json(r#"["Type(n/a, a)"]"#, r#"["Type(n/a, a)"]"#)),
        Err(ErrorDeltaJson::Solapamiento)
    ));
    assert!(matches!(
        parsear_delta_json_v1(&json(r#"["Type(n/a)"]"#, "[]")),
        Err(ErrorDeltaJson::AtomoInvalido {
            lado: LadoDelta::Add,
            indice: 0,
        })
    ));
    let error = parsear_delta_json_v1(&json(r#"["Type(n/PRIVATE_PATH)"]"#, "[]")).unwrap_err();
    assert!(!format!("{error} {error:?}").contains("PRIVATE_PATH"));
    let error =
        parsear_delta_json_v1(&json(r#"["PRIVATE_EXT(n/PRIVATE_PATH)"]"#, "[]")).unwrap_err();
    let redactado = format!("{error} {error:?}");
    assert!(!redactado.contains("PRIVATE_EXT"));
    assert!(!redactado.contains("PRIVATE_PATH"));
    let error = parsear_delta_json_v1(&json(r#"["HasLab(n/a, \"PRIVATE_LABEL\" extra)"]"#, "[]"))
        .unwrap_err();
    assert!(!format!("{error} {error:?}").contains("PRIVATE_LABEL"));
    let error =
        parsear_delta_json_v1(&json(r#"["Type(n/a, a)","Type(n/a, \u0061)"]"#, "[]")).unwrap_err();
    assert!(matches!(error, ErrorDeltaJson::AtomoDuplicado { .. }));
    assert!(!format!("{error} {error:?}").contains("Type(n/a"));
    let error = parsear_delta_json_v1(&json(
        r#"["Type(n/PRIVATE_DELTA, a)"]"#,
        r#"["Type(n/PRIVATE_DELTA, a)"]"#,
    ))
    .unwrap_err();
    assert!(matches!(error, ErrorDeltaJson::Solapamiento));
    assert!(!format!("{error} {error:?}").contains("PRIVATE_DELTA"));
}

#[test]
fn roundtrip_con_unicode_comillas_backslash_y_etiqueta_vacia() {
    let etiqueta = Atomo::HasLab {
        sujeto: Sujeto::Nodo(BundlePath::nuevo("n/niño").unwrap()),
        etiqueta: Etiqueta::nuevo("Título con \"comillas\" y \\ barra"),
    };
    let vacia = Atomo::HasLab {
        sujeto: Sujeto::Nodo(BundlePath::nuevo("n/vacio").unwrap()),
        etiqueta: Etiqueta::nuevo(""),
    };
    let delta = Delta::nuevo([etiqueta, vacia], []).unwrap();
    let serializado = serializar_delta_json_v1(&delta).unwrap();

    assert!(serializado.contains("niño"));
    assert_eq!(parsear_delta_json_v1(&serializado).unwrap(), delta);
}

#[test]
fn parse_y_serializa_json_canonico_con_orden_tipado_determinista() {
    let entrada = json(
        r#"["Type(n/b, a)","Parent(n/b, n/a)","HasLab(n/a, \"etiqueta\")"]"#,
        r#"["Type(n/a, a)"]"#,
    );
    let delta = parsear_delta_json_v1(&entrada).unwrap();
    let esperado = r#"{"schema":"felix-delta-v1","add":["HasLab(n/a, \"etiqueta\")","Parent(n/b, n/a)","Type(n/b, a)"],"remove":["Type(n/a, a)"]}"#;

    assert_eq!(serializar_delta_json_v1(&delta).unwrap(), esperado);
    assert_eq!(parsear_delta_json_v1(esperado).unwrap(), delta);
    assert_eq!(delta.agregar().len(), 3);
    assert_eq!(delta.retirar().len(), 1);
}

#[test]
fn recorre_cargas_de_1_100_y_10000_operaciones() {
    for cantidad in [1, 100, 10_000] {
        let delta = Delta::nuevo(
            (0..cantidad).map(|i| type_atom(format!("n/item-{i:05}"))),
            std::iter::empty::<Atomo>(),
        )
        .unwrap();
        let serializado = serializar_delta_json_v1(&delta).unwrap();
        let bytes_parseados = serializado.len();
        let parseado = parsear_delta_json_v1(&serializado).unwrap();

        assert_eq!(parseado, delta, "cantidad={cantidad}");
        assert_eq!(parseado.agregar().len(), cantidad, "cantidad={cantidad}");
        assert!(parseado.retirar().is_empty());
        assert!(bytes_parseados > cantidad, "cantidad={cantidad}");
    }
}
