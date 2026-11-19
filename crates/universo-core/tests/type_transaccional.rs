use universo_core::{
    Anexo, Atomo, BundlePath, ClaseConflicto, Delta, Estructura, Etiqueta, ResultadoPropuesta,
    Sorte, construir_candidato, proponer,
};

fn p(value: &str) -> BundlePath {
    BundlePath::nuevo(value).unwrap()
}
fn ty(path: &str, sort: &str) -> Atomo {
    Atomo::Type {
        sujeto: p(path),
        sorte: Sorte::nuevo(sort),
    }
}
fn base() -> Estructura {
    let mut base = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let a = base
        .insertar_nodo(p("n/a"), Sorte::nuevo("a"), Etiqueta::nuevo("label-a"))
        .unwrap();
    let b = base
        .insertar_nodo(p("n/b"), Sorte::nuevo("b"), Etiqueta::nuevo("label-b"))
        .unwrap();
    base.asignar_padre(b, a).unwrap();
    base.insertar_arista(a, b, Sorte::nuevo("rel"), Etiqueta::nuevo("edge"))
        .unwrap();
    base.asignar_atributo(a, "Nivel", "alto").unwrap();
    base.anexar(
        a,
        Anexo {
            cuerpo: "PRIVATE_BODY".into(),
            passthrough: vec![],
        },
    )
    .unwrap();
    base
}

#[test]
fn cambiar_type_preserva_identidades_relaciones_atributos_y_anexo() {
    let base = base();
    let before = base.atomos();
    let id = base.por_path(&p("n/a")).unwrap();
    let delta = Delta::nuevo([ty("n/a", "b")], [ty("n/a", "a")]).unwrap();
    let result = construir_candidato(&base, &delta).unwrap();
    let candidate = result.estructura();
    assert_eq!(candidate.por_path(&p("n/a")), Some(id));
    assert_eq!(candidate.nodo(id).unwrap().tipo, Sorte::nuevo("b"));
    let expected: std::collections::BTreeSet<_> = before
        .iter()
        .filter(|atom| **atom != ty("n/a", "a"))
        .cloned()
        .chain([ty("n/a", "b")])
        .collect();
    assert_eq!(
        candidate
            .atomos()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
    assert_eq!(candidate.anexo(id), base.anexo(id));
    assert_eq!(base.atomos(), before);
    candidate.verificar_representacion().unwrap();
}

#[test]
fn alta_identica_noop_y_cambio_requiere_retiro_exacto() {
    let base = base();
    let noop = Delta::nuevo([ty("n/a", "a")], []).unwrap();
    assert_eq!(
        construir_candidato(&base, &noop)
            .unwrap()
            .estructura()
            .atomos(),
        base.atomos()
    );
    let conflict = Delta::nuevo([ty("n/a", "b")], []).unwrap();
    let ResultadoPropuesta::RechazadaConflictos(result) = proponer(&base, &conflict, &[]).unwrap()
    else {
        panic!("conflicto esperado")
    };
    assert!(
        result
            .diagnosticos()
            .iter()
            .any(|d| d.clase() == ClaseConflicto::Type && d.cantidad() == 1)
    );
}

#[test]
fn errores_tecnicos_no_eliminan_ni_crean_identidades_y_no_filtran_valores() {
    let base = base();
    let before = base.atomos();
    for (add, remove, expected) in [
        (vec![], vec![ty("n/a", "a")], "type_replacement_required"),
        (
            vec![ty("n/a", "b")],
            vec![ty("n/a", "PRIVATE_WRONG")],
            "remove_not_found",
        ),
        (
            vec![ty("n/a", "PRIVATE_SORT")],
            vec![ty("n/a", "a")],
            "unknown_node_sort",
        ),
        (vec![ty("n/PRIVATE_NEW", "a")], vec![], "subject_not_found"),
    ] {
        let delta = Delta::nuevo(add, remove).unwrap();
        let error = construir_candidato(&base, &delta).unwrap_err();
        assert_eq!(error.codigo(), expected);
        assert!(!format!("{error:?} {error}").contains("PRIVATE"));
        assert_eq!(base.atomos(), before);
    }
}

#[test]
fn invariantes_se_evaluan_sobre_tipo_nuevo_y_no_sobre_snapshot() {
    let base = base();
    let delta = Delta::nuevo([ty("n/a", "b")], [ty("n/a", "a")]).unwrap();
    let yes = universo_core::sexpr::parsear_programa("(Type @n/a b)").unwrap();
    let no = universo_core::sexpr::parsear_programa("(Type @n/a a)").unwrap();
    assert!(matches!(
        proponer(&base, &delta, &yes).unwrap(),
        ResultadoPropuesta::Admitida(_)
    ));
    assert!(matches!(
        proponer(&base, &delta, &no).unwrap(),
        ResultadoPropuesta::RechazadaInvariantes(_)
    ));
    assert_eq!(
        base.nodo(base.por_path(&p("n/a")).unwrap()).unwrap().tipo,
        Sorte::nuevo("a")
    );
}

#[test]
fn multiples_tipos_finales_son_conflicto_y_no_depende_del_orden() {
    let base = base();
    let a = Delta::nuevo([ty("n/a", "b"), ty("n/a", "Retirado")], [ty("n/a", "a")]).unwrap();
    let b = Delta::nuevo([ty("n/a", "Retirado"), ty("n/a", "b")], [ty("n/a", "a")]).unwrap();
    for delta in [a, b] {
        assert!(matches!(
            proponer(&base, &delta, &[]).unwrap(),
            ResultadoPropuesta::RechazadaConflictos(_)
        ));
    }
}

#[test]
fn conflicto_type_no_oculta_retiro_invalido_en_otra_operacion() {
    let base = base();
    let delta = Delta::nuevo(
        [ty("n/a", "b")],
        [Atomo::Parent {
            hijo: p("n/b"),
            padre: p("n/PRIVATE_ABSENT"),
        }],
    )
    .unwrap();
    let error = proponer(&base, &delta, &[]).unwrap_err();
    assert_eq!(error.codigo(), "remove_not_found");
    assert_eq!(error.clase(), Some(universo_core::ClaseOperacion::Parent));
}
