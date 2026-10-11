use universo_core::{BundlePath, ErrorManifiesto, Manifiesto};
use universo_okf::{Regla, parsear_bundle, parsear_manifiesto};
use universo_testkit::manifiesto_minimo;

fn doc(titulo: &str, extra: &str) -> String {
    format!("---\ntype: a\ntitle: {titulo}\n{extra}---\n")
}

fn manifiesto_con_parent_key() -> Manifiesto {
    let mut manifiesto = manifiesto_minimo();
    manifiesto.layout.parent_key = Some("parent".into());
    manifiesto
}

fn contiene_parent(u: &universo_core::Estructura, hijo: &str, padre: &str) -> bool {
    let esperado = format!("Parent({hijo}, {padre})");
    u.atomos().iter().any(|atomo| atomo.to_string() == esperado)
}

#[test]
fn manifiesto_sin_parent_key_conserva_inferencia_y_passthrough_legacy() {
    let archivos = vec![
        ("n/index.md".into(), doc("Raíz", "")),
        ("n/sub/index.md".into(), doc("Sub", "")),
        (
            "n/sub/hoja.md".into(),
            doc("Hoja", "parent: n/no-aplica\nextra: conservar\n"),
        ),
    ];

    let u = parsear_bundle(&archivos, manifiesto_minimo()).unwrap();
    assert!(contiene_parent(&u, "n/sub/index", "n/index"));
    assert!(contiene_parent(&u, "n/sub/hoja", "n/sub/index"));
    let hoja = u
        .por_path(&BundlePath::nuevo("n/sub/hoja").unwrap())
        .unwrap();
    let passthrough: Vec<&str> = u
        .anexo(hoja)
        .unwrap()
        .passthrough
        .iter()
        .map(|entrada| entrada.clave.as_str())
        .collect();
    assert_eq!(passthrough, ["parent", "extra"]);
}

#[test]
fn parent_explicito_resuelve_bundlepath_unicode_y_exactamente() {
    let archivos = vec![
        ("n/index.md".into(), doc("Raíz", "")),
        ("n/Árbol/Índice.md".into(), doc("Índice", "")),
        ("n/hoja.md".into(), doc("Hoja", "parent: n/Árbol/Índice\n")),
    ];

    let u = parsear_bundle(&archivos, manifiesto_con_parent_key()).unwrap();
    assert!(contiene_parent(&u, "n/hoja", "n/Árbol/Índice"));
    let hoja = u.por_path(&BundlePath::nuevo("n/hoja").unwrap()).unwrap();
    assert!(u.anexo(hoja).unwrap().passthrough.is_empty());
}

#[test]
fn parent_explicito_no_normaliza_case_y_error_no_revela_la_referencia() {
    let secreto = "n/INDEX";
    let archivos = vec![
        ("n/index.md".into(), doc("Raíz", "")),
        (
            "n/hoja.md".into(),
            doc("Hoja", &format!("parent: {secreto}\n")),
        ),
    ];

    let errores = parsear_bundle(&archivos, manifiesto_con_parent_key()).unwrap_err();
    assert_eq!(errores.len(), 1);
    assert_eq!(errores[0].regla, Regla::ParentNoEncontrado);
    assert_eq!(errores[0].path, None);
    let visible = format!("{} {:?}", errores[0], errores[0]);
    assert!(!visible.contains(secreto), "error sin redactar: {visible}");
}

#[test]
fn parent_vacio_declara_raiz_y_campo_ausente_usa_inferencia_legacy() {
    let archivos = vec![
        ("n/index.md".into(), doc("Raíz", "")),
        ("n/sub/index.md".into(), doc("Sub", "parent: \"\"\n")),
        ("n/sub/hoja.md".into(), doc("Hoja", "")),
    ];

    let u = parsear_bundle(&archivos, manifiesto_con_parent_key()).unwrap();
    assert!(!contiene_parent(&u, "n/sub/index", "n/index"));
    assert!(contiene_parent(&u, "n/sub/hoja", "n/sub/index"));
    let sub = u
        .por_path(&BundlePath::nuevo("n/sub/index").unwrap())
        .unwrap();
    assert!(u.anexo(sub).unwrap().passthrough.is_empty());
}

#[test]
fn parent_self_y_ciclo_se_rechazan_sin_exponer_paths() {
    let casos = [
        (
            vec![(
                "n/PRIVATE_SELF.md".into(),
                doc("Self", "parent: n/PRIVATE_SELF\n"),
            )],
            Regla::ParentDeSiMismo,
            "PRIVATE_SELF",
        ),
        (
            vec![
                ("n/PRIVATE_A.md".into(), doc("A", "parent: n/PRIVATE_B\n")),
                ("n/PRIVATE_B.md".into(), doc("B", "parent: n/PRIVATE_A\n")),
            ],
            Regla::ParentCiclo,
            "PRIVATE_",
        ),
    ];

    for (archivos, regla, secreto) in casos {
        let errores = parsear_bundle(&archivos, manifiesto_con_parent_key()).unwrap_err();
        assert!(
            errores.iter().any(|error| error.regla == regla),
            "{errores:?}"
        );
        let visible = format!("{errores:?}");
        assert!(!visible.contains(secreto), "error sin redactar: {visible}");
        assert!(errores.iter().all(|error| error.path.is_none()));
    }
}

#[test]
fn parent_invalido_o_duplicado_se_rechaza_sin_exponer_el_valor() {
    for (extra, regla, secreto) in [
        ("parent: ../PRIVATE\n", Regla::ParentInvalido, "PRIVATE"),
        (
            "parent: n/PRIVATE_A\nparent: n/PRIVATE_B\n",
            Regla::ParentDuplicado,
            "PRIVATE_",
        ),
    ] {
        let archivos = vec![("n/hoja.md".into(), doc("Hoja", extra))];
        let errores = parsear_bundle(&archivos, manifiesto_con_parent_key()).unwrap_err();
        assert!(
            errores.iter().any(|error| error.regla == regla),
            "{errores:?}"
        );
        let visible = format!("{errores:?}");
        assert!(!visible.contains(secreto), "error sin redactar: {visible}");
        assert!(errores.iter().all(|error| error.path.is_none()));
    }
}

#[test]
fn parent_key_toml_es_optativo_y_se_valida_contra_claves_administradas() {
    let legado = "version = \"x/1\"\n[nodos]\nsortes = [\"a\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n";
    let configurado = legado.replace(
        "directorios = [\"n\"]",
        "directorios = [\"n\"]\nparent_key = \"parent\"",
    );
    assert_eq!(parsear_manifiesto(legado).unwrap().layout.parent_key, None);
    assert_eq!(
        parsear_manifiesto(&configurado)
            .unwrap()
            .layout
            .parent_key
            .as_deref(),
        Some("parent")
    );

    let mut manifiesto = manifiesto_minimo();
    manifiesto.layout.parent_key = Some(String::new());
    assert!(
        manifiesto
            .validar()
            .unwrap_err()
            .contains(&ErrorManifiesto::ParentKeyVacia)
    );

    for (clave, esperado) in [
        ("title", ErrorManifiesto::ClaveReservada("title".into())),
        ("type", ErrorManifiesto::ClaveReservada("type".into())),
        ("status", ErrorManifiesto::ClaveReservada("status".into())),
        ("nivel", ErrorManifiesto::ClaveDuplicada("nivel".into())),
        ("rel", ErrorManifiesto::ClaveDuplicada("rel".into())),
    ] {
        let mut manifiesto = manifiesto_minimo();
        manifiesto.layout.parent_key = Some(clave.into());
        assert!(manifiesto.validar().unwrap_err().contains(&esperado));
    }
}

#[test]
fn parent_key_usa_la_misma_gramatica_ascii_que_el_parser_frontmatter() {
    for clave in ["parent_2", "Parent-2", "_parent"] {
        let mut manifiesto = manifiesto_minimo();
        manifiesto.layout.parent_key = Some(clave.into());
        manifiesto.validar().unwrap();
    }

    for clave in ["1parent", "parent/key", "parent.key", "áparent"] {
        let mut manifiesto = manifiesto_minimo();
        manifiesto.layout.parent_key = Some(clave.into());
        assert!(
            manifiesto
                .validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::ParentKeyInvalida),
            "la clave {clave:?} no respeta la gramática del parser"
        );

        let texto = format!(
            "version = \"x/1\"\n[nodos]\nsortes = [\"a\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\nparent_key = {clave:?}"
        );
        assert!(parsear_manifiesto(&texto).is_err());
    }
}
