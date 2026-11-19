//! `felix-delta-v2`: notas nuevas (`create`) más átomos, y despacho por schema.
use universo_core::{BundlePath, Delta, Etiqueta, NotaNueva, Sorte};
use universo_okf::{
    ErrorDeltaJson, parsear_delta_json, parsear_delta_json_v1, parsear_delta_json_v2,
    serializar_delta_json_v2,
};

const V2: &str = r#"{"schema":"felix-delta-v2","create":[{"path":"n/z","type":"a","title":"Zeta","body":"Texto.\n"},{"path":"n/a","type":"b","title":"Alfa"}],"add":["Nivel(n/z, alto)"],"remove":[]}"#;

#[test]
fn parsea_notas_y_atomos_con_notas_ordenadas() {
    let delta = parsear_delta_json_v2(V2).unwrap();
    let paths: Vec<_> = delta.crear().iter().map(|n| n.path.como_str()).collect();
    assert_eq!(paths, vec!["n/a", "n/z"]);
    assert_eq!(delta.crear()[0].cuerpo, "");
    assert_eq!(delta.crear()[1].cuerpo, "Texto.\n");
    assert_eq!(delta.crear()[1].tipo, Sorte::nuevo("a"));
    assert_eq!(delta.crear()[1].etiqueta, Etiqueta::nuevo("Zeta"));
    assert_eq!(delta.agregar().len(), 1);
}

#[test]
fn serializa_de_forma_determinista_y_vuelve_igual() {
    let delta = parsear_delta_json_v2(V2).unwrap();
    let texto = serializar_delta_json_v2(&delta).unwrap();
    assert_eq!(parsear_delta_json_v2(&texto).unwrap(), delta);
    assert_eq!(serializar_delta_json_v2(&delta).unwrap(), texto);
    assert!(texto.find("\"n/a\"").unwrap() < texto.find("\"n/z\"").unwrap());
}

#[test]
fn despacha_por_schema_y_conserva_v1() {
    let v1 = r#"{"schema":"felix-delta-v1","add":[],"remove":[]}"#;
    assert_eq!(
        parsear_delta_json(v1).unwrap(),
        parsear_delta_json_v1(v1).unwrap()
    );
    assert_eq!(
        parsear_delta_json(V2).unwrap(),
        parsear_delta_json_v2(V2).unwrap()
    );
    assert!(matches!(
        parsear_delta_json(r#"{"schema":"felix-delta-v9","add":[],"remove":[]}"#),
        Err(ErrorDeltaJson::SchemaNoSoportado)
    ));
    assert!(matches!(
        parsear_delta_json("no json"),
        Err(ErrorDeltaJson::Json)
    ));
    // v1 sigue sin aceptar `create`.
    assert!(
        parsear_delta_json_v1(r#"{"schema":"felix-delta-v1","create":[],"add":[],"remove":[]}"#)
            .is_err()
    );
}

#[test]
fn rechaza_claves_desconocidas_paths_invalidos_y_notas_repetidas() {
    let casos = [
        r#"{"schema":"felix-delta-v2","create":[],"add":[],"remove":[],"x":1}"#,
        r#"{"schema":"felix-delta-v2","create":[{"path":"n/x","type":"a","title":"X","extra":1}],"add":[],"remove":[]}"#,
        r#"{"schema":"felix-delta-v2","create":[{"path":"n/x","type":"a"}],"add":[],"remove":[]}"#,
    ];
    for caso in casos {
        assert!(
            matches!(parsear_delta_json_v2(caso), Err(ErrorDeltaJson::Json)),
            "{caso}"
        );
    }
    assert!(matches!(
        parsear_delta_json_v2(
            r#"{"schema":"felix-delta-v2","create":[{"path":"../fuera","type":"a","title":"X"}],"add":[],"remove":[]}"#
        ),
        Err(ErrorDeltaJson::NotaInvalida { indice: 0 })
    ));
    assert!(matches!(
        parsear_delta_json_v2(
            r#"{"schema":"felix-delta-v2","create":[{"path":"n/x","type":"a","title":"X"},{"path":"n/X","type":"a","title":"Y"}],"add":[],"remove":[]}"#
        ),
        Err(ErrorDeltaJson::NotaDuplicada { .. })
    ));
}

#[test]
fn v2_sin_notas_equivale_a_v1() {
    let v2 = r#"{"schema":"felix-delta-v2","create":[],"add":["Type(n/x, a)"],"remove":[]}"#;
    let v1 = r#"{"schema":"felix-delta-v1","add":["Type(n/x, a)"],"remove":[]}"#;
    assert_eq!(
        parsear_delta_json_v2(v2).unwrap(),
        parsear_delta_json_v1(v1).unwrap()
    );
    let _ = (
        Delta::default(),
        NotaNueva {
            path: BundlePath::nuevo("n/x").unwrap(),
            tipo: Sorte::nuevo("a"),
            etiqueta: Etiqueta::nuevo("X"),
            cuerpo: String::new(),
        },
    );
}

#[test]
fn update_de_cuerpos_se_parsea_serializa_y_es_canonico() {
    let texto = r#"{"schema":"felix-delta-v2","create":[],"update":[{"path":"n/z","body":"Z\n"},{"path":"n/a","body":""}],"add":[],"remove":[]}"#;
    let delta = parsear_delta_json_v2(texto).unwrap();
    let paths: Vec<_> = delta.cuerpos().iter().map(|c| c.path.como_str()).collect();
    assert_eq!(paths, vec!["n/a", "n/z"]);
    assert_eq!(delta.cuerpos()[1].cuerpo, "Z\n");
    let canonico = universo_okf::serializar_delta_json_canonico(&delta).unwrap();
    assert!(canonico.contains("\"update\""), "{canonico}");
    assert_eq!(parsear_delta_json(&canonico).unwrap(), delta);
    assert!(universo_okf::serializar_delta_json_v1(&delta).is_err());
    // Sin update, la clave no aparece (forma canónica estable).
    let solo_notas = serializar_delta_json_v2(&parsear_delta_json_v2(V2).unwrap()).unwrap();
    assert!(!solo_notas.contains("update"));
    assert!(matches!(
        parsear_delta_json_v2(
            r#"{"schema":"felix-delta-v2","create":[],"update":[{"path":"n/a","body":"x"},{"path":"n/A","body":"y"}],"add":[],"remove":[]}"#
        ),
        Err(ErrorDeltaJson::CuerpoDuplicado { .. })
    ));
}
