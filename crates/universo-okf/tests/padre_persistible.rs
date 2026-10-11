use universo_core::{
    Atomo, BundlePath, Candidato, Delta, ErrorManifiesto, Manifiesto, construir_candidato,
};
use universo_okf::{
    DocumentoCandidato, ParentOverride, Regla, parsear_bundle, parsear_manifiesto,
    serializar_candidato, serializar_nodo,
};
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

fn parent(hijo: &str, padre: &str) -> Atomo {
    Atomo::Parent {
        hijo: BundlePath::nuevo(hijo).unwrap(),
        padre: BundlePath::nuevo(padre).unwrap(),
    }
}

fn construir(snapshot: &universo_core::Estructura, delta: &Delta) -> Candidato {
    construir_candidato(snapshot, delta).unwrap()
}

fn archivos_de(documentos: &[DocumentoCandidato]) -> Vec<(String, String)> {
    documentos
        .iter()
        .map(|documento| {
            (
                format!("{}.md", documento.path().como_str()),
                documento.contenido().to_string(),
            )
        })
        .collect()
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

#[test]
fn serializar_y_recargar_conserva_parent_explicito_unicode_y_case() {
    let archivos = vec![
        ("n/Índice.md".into(), doc("Índice", "")),
        ("n/hoja.md".into(), doc("Hoja", "parent: n/Índice\n")),
    ];
    let original = parsear_bundle(&archivos, manifiesto_con_parent_key()).unwrap();
    let serializados = original
        .nodos()
        .map(|(id, nodo)| {
            (
                format!("{}.md", nodo.path.como_str()),
                serializar_nodo(&original, id),
            )
        })
        .collect::<Vec<_>>();
    let recargado = parsear_bundle(&serializados, manifiesto_con_parent_key()).unwrap();

    assert!(contiene_parent(&recargado, "n/hoja", "n/Índice"));
    let hoja = recargado
        .por_path(&BundlePath::nuevo("n/hoja").unwrap())
        .unwrap();
    assert!(serializar_nodo(&recargado, hoja).contains("parent: n/Índice\n"));
}

#[test]
fn serializar_y_recargar_conserva_raiz_explicita_y_ausencia_legacy() {
    let archivos = vec![
        ("n/index.md".into(), doc("Raíz", "")),
        ("n/sub/index.md".into(), doc("Sub", "parent: \"\"\n")),
        ("n/sub/hoja.md".into(), doc("Hoja", "")),
    ];
    let original = parsear_bundle(&archivos, manifiesto_con_parent_key()).unwrap();
    let serializados = original
        .nodos()
        .map(|(id, nodo)| {
            (
                format!("{}.md", nodo.path.como_str()),
                serializar_nodo(&original, id),
            )
        })
        .collect::<Vec<_>>();
    let recargado = parsear_bundle(&serializados, manifiesto_con_parent_key()).unwrap();

    assert!(!contiene_parent(&recargado, "n/sub/index", "n/index"));
    assert!(contiene_parent(&recargado, "n/sub/hoja", "n/sub/index"));
    let sub = recargado
        .por_path(&BundlePath::nuevo("n/sub/index").unwrap())
        .unwrap();
    let hoja = recargado
        .por_path(&BundlePath::nuevo("n/sub/hoja").unwrap())
        .unwrap();
    assert!(serializar_nodo(&recargado, sub).contains("parent: \"\"\n"));
    assert!(!serializar_nodo(&recargado, hoja).contains("parent:"));
}

#[test]
fn candidato_parent_sin_parent_key_falla_con_codigo_estable_y_redactado() {
    let archivos = vec![
        ("n/sub/index.md".into(), doc("Índice", "")),
        ("n/sub/hoja.md".into(), doc("Hoja", "")),
    ];
    let snapshot = parsear_bundle(&archivos, manifiesto_minimo()).unwrap();
    assert!(contiene_parent(&snapshot, "n/sub/hoja", "n/sub/index"));
    let delta = Delta::nuevo([], [parent("n/sub/hoja", "n/sub/index")]).unwrap();
    let candidato = construir(&snapshot, &delta);

    let error = serializar_candidato(&snapshot, &candidato, &[]).unwrap_err();
    assert_eq!(error.codigo(), "parent_not_persistable");
    let visible = format!("{error} {error:?}");
    assert!(!visible.contains("n/sub/hoja"));
    assert!(!visible.contains("n/sub/index"));
}

#[test]
fn candidato_parent_exige_override_para_cada_cambio() {
    let archivos = vec![
        ("n/index.md".into(), doc("Índice", "")),
        ("n/hoja.md".into(), doc("Hoja", "parent: n/index\n")),
    ];
    let snapshot = parsear_bundle(&archivos, manifiesto_con_parent_key()).unwrap();
    let delta = Delta::nuevo([], [parent("n/hoja", "n/index")]).unwrap();
    let candidato = construir(&snapshot, &delta);

    let error = serializar_candidato(&snapshot, &candidato, &[]).unwrap_err();
    assert_eq!(error.codigo(), "parent_override_required");
}

#[test]
fn candidato_rechaza_overrides_sobrantes_y_duplicados_sin_revelar_paths() {
    let archivos = vec![
        ("n/PRIVATE_parent.md".into(), doc("Padre", "")),
        (
            "n/PRIVATE_child.md".into(),
            doc("Hijo", "parent: n/PRIVATE_parent\n"),
        ),
    ];
    let snapshot = parsear_bundle(&archivos, manifiesto_con_parent_key()).unwrap();
    let delta = Delta::nuevo(std::iter::empty::<Atomo>(), std::iter::empty()).unwrap();
    let candidato = construir(&snapshot, &delta);

    for duplicado in [false, true] {
        let mut overrides = vec![ParentOverride::new(
            BundlePath::nuevo("n/PRIVATE_child").unwrap(),
            Some(BundlePath::nuevo("n/PRIVATE_parent").unwrap()),
        )];
        if duplicado {
            overrides.push(ParentOverride::new(
                BundlePath::nuevo("n/PRIVATE_child").unwrap(),
                Some(BundlePath::nuevo("n/PRIVATE_parent").unwrap()),
            ));
        }

        let error = serializar_candidato(&snapshot, &candidato, &overrides).unwrap_err();
        assert_eq!(error.codigo(), "parent_override_unexpected");
        let visible = format!("{error} {error:?} {overrides:?}");
        assert!(
            !visible.contains("PRIVATE"),
            "error sin redactar: {visible}"
        );
    }
}

#[test]
fn candidato_rechaza_snapshot_incompatible_sin_revelar_paths() {
    let archivos_base = vec![("n/PRIVATE_base.md".into(), doc("Base", ""))];
    let snapshot = parsear_bundle(&archivos_base, manifiesto_minimo()).unwrap();
    let delta = Delta::nuevo(std::iter::empty::<Atomo>(), std::iter::empty()).unwrap();

    let candidato_manifest = construir(
        &parsear_bundle(&archivos_base, manifiesto_con_parent_key()).unwrap(),
        &delta,
    );
    let candidato_nodos = construir(
        &parsear_bundle(
            &[("n/PRIVATE_otro.md".into(), doc("Otro", ""))],
            manifiesto_minimo(),
        )
        .unwrap(),
        &delta,
    );

    for candidato in [candidato_manifest, candidato_nodos] {
        let error = serializar_candidato(&snapshot, &candidato, &[]).unwrap_err();
        assert_eq!(error.codigo(), "snapshot_incompatible");
        let visible = format!("{error} {error:?}");
        assert!(
            !visible.contains("PRIVATE"),
            "error sin redactar: {visible}"
        );
    }
}

#[test]
fn candidato_roundtrip_conserva_parent_explicito_e_inferido_en_hermanos() {
    let archivos = vec![
        ("n/index.md".into(), doc("Raíz", "")),
        ("n/sub/index.md".into(), doc("Sub", "parent: \"\"\n")),
        (
            "n/sub/explicito.md".into(),
            doc("Explícito", "parent: n/sub/index\n"),
        ),
        ("n/sub/inferido.md".into(), doc("Inferido", "")),
        (
            "n/sub/cambio.md".into(),
            doc("Cambio", "parent: n/sub/index\n"),
        ),
    ];
    let snapshot = parsear_bundle(&archivos, manifiesto_con_parent_key()).unwrap();
    let delta = Delta::nuevo(
        [parent("n/sub/cambio", "n/index")],
        [parent("n/sub/cambio", "n/sub/index")],
    )
    .unwrap();
    let candidato = construir(&snapshot, &delta);
    let override_parent = ParentOverride::new(
        BundlePath::nuevo("n/sub/cambio").unwrap(),
        Some(BundlePath::nuevo("n/index").unwrap()),
    );
    let documentos = serializar_candidato(&snapshot, &candidato, &[override_parent]).unwrap();

    let explicito = documentos
        .iter()
        .find(|documento| documento.path().como_str() == "n/sub/explicito")
        .unwrap();
    let inferido = documentos
        .iter()
        .find(|documento| documento.path().como_str() == "n/sub/inferido")
        .unwrap();
    assert!(explicito.contenido().contains("parent: n/sub/index\n"));
    assert!(!inferido.contenido().contains("parent:"));

    let recargado = parsear_bundle(&archivos_de(&documentos), manifiesto_con_parent_key()).unwrap();
    assert!(contiene_parent(
        &recargado,
        "n/sub/explicito",
        "n/sub/index"
    ));
    assert!(contiene_parent(&recargado, "n/sub/inferido", "n/sub/index"));
    let explicito_id = recargado
        .por_path(&BundlePath::nuevo("n/sub/explicito").unwrap())
        .unwrap();
    let inferido_id = recargado
        .por_path(&BundlePath::nuevo("n/sub/inferido").unwrap())
        .unwrap();
    assert!(recargado.parent_frontmatter_explicito(explicito_id));
    assert!(!recargado.parent_frontmatter_explicito(inferido_id));
}

#[test]
fn candidato_parent_reemplaza_y_quita_varios_hijos_con_roundtrip_determinista() {
    let archivos = vec![
        ("n/Old.md".into(), doc("Viejo", "")),
        ("n/Árbol/Índice.md".into(), doc("Índice", "")),
        (
            "n/sub/reemplazo.md".into(),
            doc("Reemplazo", "parent: n/Old\n"),
        ),
        (
            "n/sub/retirado.md".into(),
            doc("Retirado", "parent: n/Old\n"),
        ),
    ];
    let snapshot = parsear_bundle(&archivos, manifiesto_con_parent_key()).unwrap();
    let delta = Delta::nuevo(
        [parent("n/sub/reemplazo", "n/Árbol/Índice")],
        [
            parent("n/sub/reemplazo", "n/Old"),
            parent("n/sub/retirado", "n/Old"),
        ],
    )
    .unwrap();
    let candidato = construir(&snapshot, &delta);

    let override_mal_case = ParentOverride::new(
        BundlePath::nuevo("n/sub/reemplazo").unwrap(),
        Some(BundlePath::nuevo("n/árbol/índice").unwrap()),
    );
    let error = serializar_candidato(
        &snapshot,
        &candidato,
        &[
            override_mal_case,
            ParentOverride::new(BundlePath::nuevo("n/sub/retirado").unwrap(), None),
        ],
    )
    .unwrap_err();
    assert_eq!(error.codigo(), "parent_override_mismatch");
    let visible = format!("{error} {error:?}");
    assert!(!visible.contains("n/sub/reemplazo"));
    assert!(!visible.contains("n/árbol/índice"));

    let overrides = [
        ParentOverride::new(
            BundlePath::nuevo("n/sub/reemplazo").unwrap(),
            Some(BundlePath::nuevo("n/Árbol/Índice").unwrap()),
        ),
        ParentOverride::new(BundlePath::nuevo("n/sub/retirado").unwrap(), None),
    ];
    let primera = serializar_candidato(&snapshot, &candidato, &overrides).unwrap();
    let segunda = serializar_candidato(&snapshot, &candidato, &overrides).unwrap();
    assert_eq!(primera, segunda);
    let texto_debug = format!("{primera:?} {overrides:?}");
    assert!(!texto_debug.contains("n/sub/reemplazo"));
    assert!(!texto_debug.contains("parent: n/Árbol/Índice"));

    let reemplazo = primera
        .iter()
        .find(|documento| documento.path().como_str() == "n/sub/reemplazo")
        .unwrap();
    let retirado = primera
        .iter()
        .find(|documento| documento.path().como_str() == "n/sub/retirado")
        .unwrap();
    assert!(reemplazo.contenido().contains("parent: n/Árbol/Índice\n"));
    assert!(retirado.contenido().contains("parent: \"\"\n"));

    let recargado = parsear_bundle(&archivos_de(&primera), manifiesto_con_parent_key()).unwrap();
    assert!(contiene_parent(
        &recargado,
        "n/sub/reemplazo",
        "n/Árbol/Índice"
    ));
    assert!(!contiene_parent(&recargado, "n/sub/retirado", "n/Old"));
    let delta_vacio = Delta::nuevo(std::iter::empty::<Atomo>(), std::iter::empty()).unwrap();
    let recargado_candidato = construir(&recargado, &delta_vacio);
    let idempotente = serializar_candidato(&recargado, &recargado_candidato, &[]).unwrap();
    assert_eq!(primera, idempotente);
}
