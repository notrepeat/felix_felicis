use universo_core::{
    Atomo, BundlePath, ClaseConflicto, ClaseOperacion, Delta, ErrorCandidato, ErrorDelta,
    Estructura, Etiqueta, IdArista, Sorte, Sujeto, detectar_conflictos_funcionales,
};

fn p(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn nodo(estructura: &mut Estructura, path: &str, tipo: &str, etiqueta: &str) {
    estructura
        .insertar_nodo(p(path), Sorte::nuevo(tipo), Etiqueta::nuevo(etiqueta))
        .unwrap();
}

fn id_arista(origen: &str, destino: &str, tipo: &str, ordinal: u32) -> IdArista {
    IdArista {
        origen: p(origen),
        sorte: Sorte::nuevo(tipo),
        destino: p(destino),
        ordinal,
    }
}

fn cantidad(resultado: &universo_core::ResultadoConflictos, clase: ClaseConflicto) -> usize {
    resultado
        .diagnosticos()
        .iter()
        .find(|diagnostico| diagnostico.clase() == clase)
        .map_or(0, |diagnostico| diagnostico.cantidad())
}

#[test]
fn detecta_type_haslab_de_nodo_y_parent_por_clave_funcional() {
    let mut snapshot = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    nodo(
        &mut snapshot,
        "secreto/PRIVATE_PATH_A",
        "a",
        "PRIVATE_NODE_LABEL_OLD",
    );
    nodo(&mut snapshot, "secreto/PRIVATE_PATH_B", "b", "etiqueta-b");
    nodo(&mut snapshot, "secreto/PRIVATE_PATH_C", "a", "etiqueta-c");
    let a = snapshot.por_path(&p("secreto/PRIVATE_PATH_A")).unwrap();
    let b = snapshot.por_path(&p("secreto/PRIVATE_PATH_B")).unwrap();
    snapshot.asignar_padre(a, b).unwrap();
    snapshot
        .insertar_arista(
            a,
            b,
            Sorte::nuevo("rel"),
            Etiqueta::nuevo("PRIVATE_EDGE_LABEL_OLD"),
        )
        .unwrap();
    let antes = snapshot.atomos();
    let delta = Delta::nuevo(
        [
            Atomo::Type {
                sujeto: p("secreto/PRIVATE_PATH_A"),
                sorte: Sorte::nuevo("b"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Nodo(p("secreto/PRIVATE_PATH_A")),
                etiqueta: Etiqueta::nuevo("PRIVATE_NODE_LABEL_NEW"),
            },
            Atomo::Parent {
                hijo: p("secreto/PRIVATE_PATH_A"),
                padre: p("secreto/PRIVATE_PATH_C"),
            },
        ],
        [],
    )
    .unwrap();

    let resultado = detectar_conflictos_funcionales(&snapshot, &delta).unwrap();

    assert_eq!(cantidad(&resultado, ClaseConflicto::Type), 1);
    assert_eq!(cantidad(&resultado, ClaseConflicto::HasLab), 1);
    assert_eq!(cantidad(&resultado, ClaseConflicto::Parent), 1);
    assert_eq!(resultado.total_conflictos(), 3);
    assert_eq!(
        resultado
            .diagnosticos()
            .iter()
            .map(|diagnostico| diagnostico.clase())
            .collect::<Vec<_>>(),
        [
            ClaseConflicto::Type,
            ClaseConflicto::HasLab,
            ClaseConflicto::Parent
        ]
    );
    let salida = format!("{resultado} {resultado:?}");
    for canario in [
        "PRIVATE_PATH",
        "PRIVATE_NODE_LABEL",
        "PRIVATE_ATOMO",
        "PRIVATE_DELTA",
    ] {
        assert!(!salida.contains(canario), "se filtró {canario}: {salida}");
    }
    assert_eq!(snapshot.atomos(), antes);
}

#[test]
fn reemplazos_exactos_remove_add_no_son_conflictos() {
    let mut snapshot = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    nodo(&mut snapshot, "n/hijo", "a", "label-viejo");
    nodo(&mut snapshot, "n/padre-viejo", "b", "label");
    nodo(&mut snapshot, "n/padre-nuevo", "a", "label");
    let hijo = snapshot.por_path(&p("n/hijo")).unwrap();
    let padre_viejo = snapshot.por_path(&p("n/padre-viejo")).unwrap();
    snapshot.asignar_padre(hijo, padre_viejo).unwrap();
    let edge = snapshot
        .insertar_arista(
            hijo,
            padre_viejo,
            Sorte::nuevo("rel"),
            Etiqueta::nuevo("edge-vieja"),
        )
        .unwrap();
    let edge_id = id_arista(
        "n/hijo",
        "n/padre-viejo",
        "rel",
        snapshot.arista(edge).unwrap().ordinal,
    );
    let delta = Delta::nuevo(
        [
            Atomo::Type {
                sujeto: p("n/hijo"),
                sorte: Sorte::nuevo("b"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Nodo(p("n/hijo")),
                etiqueta: Etiqueta::nuevo("label-nuevo"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Arista(edge_id.clone()),
                etiqueta: Etiqueta::nuevo("edge-nueva"),
            },
            Atomo::Parent {
                hijo: p("n/hijo"),
                padre: p("n/padre-nuevo"),
            },
        ],
        [
            Atomo::Type {
                sujeto: p("n/hijo"),
                sorte: Sorte::nuevo("a"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Nodo(p("n/hijo")),
                etiqueta: Etiqueta::nuevo("label-viejo"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Arista(edge_id),
                etiqueta: Etiqueta::nuevo("edge-vieja"),
            },
            Atomo::Parent {
                hijo: p("n/hijo"),
                padre: p("n/padre-viejo"),
            },
        ],
    )
    .unwrap();
    let antes = snapshot.atomos();

    let resultado = detectar_conflictos_funcionales(&snapshot, &delta).unwrap();

    assert!(!resultado.hay_conflictos());
    assert_eq!(snapshot.atomos(), antes);
}

#[test]
fn altas_identicas_son_no_op_y_etiquetas_iguales_en_sujetos_distintos_se_permiten() {
    let mut snapshot = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    nodo(&mut snapshot, "n/a", "a", "etiqueta-comun");
    nodo(&mut snapshot, "n/b", "b", "etiqueta-comun");
    let a = snapshot.por_path(&p("n/a")).unwrap();
    let b = snapshot.por_path(&p("n/b")).unwrap();
    snapshot.asignar_padre(a, b).unwrap();
    let edge_a = snapshot
        .insertar_arista(a, b, Sorte::nuevo("rel"), Etiqueta::nuevo("arista-comun"))
        .unwrap();
    let edge_b = snapshot
        .insertar_arista(a, b, Sorte::nuevo("rel"), Etiqueta::nuevo("arista-comun"))
        .unwrap();
    let edge_a_id = id_arista(
        "n/a",
        "n/b",
        "rel",
        snapshot.arista(edge_a).unwrap().ordinal,
    );
    let edge_b_id = id_arista(
        "n/a",
        "n/b",
        "rel",
        snapshot.arista(edge_b).unwrap().ordinal,
    );
    let delta = Delta::nuevo(
        [
            Atomo::Type {
                sujeto: p("n/a"),
                sorte: Sorte::nuevo("a"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Nodo(p("n/a")),
                etiqueta: Etiqueta::nuevo("etiqueta-comun"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Nodo(p("n/b")),
                etiqueta: Etiqueta::nuevo("etiqueta-comun"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Arista(edge_a_id),
                etiqueta: Etiqueta::nuevo("arista-comun"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Arista(edge_b_id),
                etiqueta: Etiqueta::nuevo("arista-comun"),
            },
            Atomo::Parent {
                hijo: p("n/a"),
                padre: p("n/b"),
            },
        ],
        [],
    )
    .unwrap();

    let resultado = detectar_conflictos_funcionales(&snapshot, &delta).unwrap();

    assert!(!resultado.hay_conflictos());
    assert_eq!(resultado.total_conflictos(), 0);
}

#[test]
fn diagnosticos_son_deterministas_y_ext_o_retiros_ausentes_no_son_conflictos() {
    let mut snapshot = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    nodo(&mut snapshot, "n/a", "a", "label");
    nodo(&mut snapshot, "n/b", "a", "label");
    let adds = vec![
        Atomo::Type {
            sujeto: p("n/a"),
            sorte: Sorte::nuevo("b"),
        },
        Atomo::Type {
            sujeto: p("n/b"),
            sorte: Sorte::nuevo("b"),
        },
        Atomo::Ext {
            predicado: "Nivel".into(),
            sujeto: p("n/a"),
            constante: "valor-a".into(),
        },
        Atomo::Ext {
            predicado: "Nivel".into(),
            sujeto: p("n/a"),
            constante: "valor-b".into(),
        },
    ];
    let delta_a = Delta::nuevo(adds.clone(), []).unwrap();
    let delta_b = Delta::nuevo(adds.into_iter().rev(), []).unwrap();

    let resultado_a = detectar_conflictos_funcionales(&snapshot, &delta_a).unwrap();
    let resultado_b = detectar_conflictos_funcionales(&snapshot, &delta_b).unwrap();

    assert_eq!(resultado_a, resultado_b);
    assert_eq!(format!("{resultado_a:?}"), format!("{resultado_b:?}"));
    assert_eq!(cantidad(&resultado_a, ClaseConflicto::Type), 2);
    assert_eq!(resultado_a.diagnosticos().len(), 1);

    let retiro_ausente = Delta::nuevo(
        [],
        [Atomo::Type {
            sujeto: p("n/no-existe"),
            sorte: Sorte::nuevo("a"),
        }],
    )
    .unwrap();
    assert!(
        !detectar_conflictos_funcionales(&snapshot, &retiro_ausente)
            .unwrap()
            .hay_conflictos()
    );
}

#[test]
fn incidencia_inconsistente_se_rechaza_antes_y_no_hay_conflicto_representable_en_v1() {
    let mut snapshot = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let inconsistente = Atomo::Edge {
        id: id_arista(
            "secreto/PRIVATE_ID_ORIGIN",
            "secreto/PRIVATE_DEST",
            "PRIVATE_REL",
            0,
        ),
        origen: p("secreto/PRIVATE_FIELD_ORIGIN"),
        destino: p("secreto/PRIVATE_DEST"),
        sorte: Sorte::nuevo("PRIVATE_REL"),
    };
    let error = Delta::nuevo([inconsistente], []).unwrap_err();
    assert!(matches!(error, ErrorDelta::AtomoNoRepresentable { .. }));
    let salida_error = format!("{error} {error:?}");
    for canario in [
        "PRIVATE_ID_ORIGIN",
        "PRIVATE_FIELD_ORIGIN",
        "PRIVATE_DEST",
        "PRIVATE_REL",
    ] {
        assert!(!salida_error.contains(canario));
    }

    nodo(&mut snapshot, "n/origen", "a", "origen");
    nodo(&mut snapshot, "n/otro-origen", "b", "otro-origen");
    nodo(&mut snapshot, "n/destino", "a", "destino");
    let aristas_validas = Delta::nuevo(
        [
            Atomo::Edge {
                id: id_arista("n/origen", "n/destino", "rel", 0),
                origen: p("n/origen"),
                destino: p("n/destino"),
                sorte: Sorte::nuevo("rel"),
            },
            Atomo::Edge {
                id: id_arista("n/otro-origen", "n/destino", "rel", 0),
                origen: p("n/otro-origen"),
                destino: p("n/destino"),
                sorte: Sorte::nuevo("rel"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Arista(id_arista("n/origen", "n/destino", "rel", 0)),
                etiqueta: Etiqueta::nuevo("label-a"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Arista(id_arista("n/otro-origen", "n/destino", "rel", 0)),
                etiqueta: Etiqueta::nuevo("label-b"),
            },
        ],
        [],
    )
    .unwrap();
    assert!(
        !detectar_conflictos_funcionales(&snapshot, &aristas_validas)
            .unwrap()
            .hay_conflictos()
    );
}

#[test]
fn etiqueta_de_arista_se_rebasa_por_handle_tras_compactar_ordinales() {
    let mut snapshot = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    nodo(&mut snapshot, "n/origen", "a", "origen");
    nodo(&mut snapshot, "n/destino", "b", "destino");
    let origen = snapshot.por_path(&p("n/origen")).unwrap();
    let destino = snapshot.por_path(&p("n/destino")).unwrap();
    for etiqueta in ["edge-cero", "edge-uno", "edge-dos"] {
        snapshot
            .insertar_arista(
                origen,
                destino,
                Sorte::nuevo("rel"),
                Etiqueta::nuevo(etiqueta),
            )
            .unwrap();
    }
    let antes = snapshot.atomos();
    let delta = Delta::nuevo(
        [Atomo::HasLab {
            sujeto: Sujeto::Arista(id_arista("n/origen", "n/destino", "rel", 1)),
            etiqueta: Etiqueta::nuevo("edge-dos-nueva"),
        }],
        [
            Atomo::Edge {
                id: id_arista("n/origen", "n/destino", "rel", 0),
                origen: p("n/origen"),
                destino: p("n/destino"),
                sorte: Sorte::nuevo("rel"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Arista(id_arista("n/origen", "n/destino", "rel", 2)),
                etiqueta: Etiqueta::nuevo("edge-dos"),
            },
        ],
    )
    .unwrap();

    let resultado = detectar_conflictos_funcionales(&snapshot, &delta).unwrap();

    assert!(!resultado.hay_conflictos());
    assert_eq!(snapshot.atomos(), antes);
}

#[test]
fn conflicto_de_etiqueta_arista_se_agrega_con_las_demas_clases() {
    let mut snapshot = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    nodo(&mut snapshot, "secreto/PRIVATE_ORIGIN", "a", "origen");
    nodo(&mut snapshot, "secreto/PRIVATE_DEST", "b", "destino");
    nodo(
        &mut snapshot,
        "secreto/PRIVATE_PARENT_OLD",
        "a",
        "parent-old",
    );
    nodo(
        &mut snapshot,
        "secreto/PRIVATE_PARENT_NEW",
        "b",
        "parent-new",
    );
    let origen = snapshot.por_path(&p("secreto/PRIVATE_ORIGIN")).unwrap();
    let destino = snapshot.por_path(&p("secreto/PRIVATE_DEST")).unwrap();
    let padre_viejo = snapshot.por_path(&p("secreto/PRIVATE_PARENT_OLD")).unwrap();
    snapshot.asignar_padre(origen, padre_viejo).unwrap();
    snapshot
        .insertar_arista(
            origen,
            destino,
            Sorte::nuevo("rel"),
            Etiqueta::nuevo("PRIVATE_OLD_LABEL"),
        )
        .unwrap();
    let antes = snapshot.atomos();
    let delta = Delta::nuevo(
        [
            Atomo::Type {
                sujeto: p("secreto/PRIVATE_ORIGIN"),
                sorte: Sorte::nuevo("b"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Nodo(p("secreto/PRIVATE_ORIGIN")),
                etiqueta: Etiqueta::nuevo("PRIVATE_NODE_LABEL_NEW"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Arista(id_arista(
                    "secreto/PRIVATE_ORIGIN",
                    "secreto/PRIVATE_DEST",
                    "rel",
                    0,
                )),
                etiqueta: Etiqueta::nuevo("PRIVATE_EDGE_LABEL_NEW"),
            },
            Atomo::Edge {
                id: id_arista("secreto/PRIVATE_ORIGIN", "secreto/PRIVATE_DEST", "rel", 1),
                origen: p("secreto/PRIVATE_ORIGIN"),
                destino: p("secreto/PRIVATE_DEST"),
                sorte: Sorte::nuevo("rel"),
            },
            Atomo::HasLab {
                sujeto: Sujeto::Arista(id_arista(
                    "secreto/PRIVATE_ORIGIN",
                    "secreto/PRIVATE_DEST",
                    "rel",
                    1,
                )),
                etiqueta: Etiqueta::nuevo("PRIVATE_EDGE_LABEL_SINGLE"),
            },
            Atomo::Parent {
                hijo: p("secreto/PRIVATE_ORIGIN"),
                padre: p("secreto/PRIVATE_PARENT_NEW"),
            },
        ],
        [],
    )
    .unwrap();
    let delta_reordenado = Delta::nuevo(
        delta.agregar().iter().rev().cloned(),
        delta.retirar().iter().rev().cloned(),
    )
    .unwrap();

    let resultado = detectar_conflictos_funcionales(&snapshot, &delta).unwrap();
    let resultado_reordenado =
        detectar_conflictos_funcionales(&snapshot, &delta_reordenado).unwrap();

    assert_eq!(resultado, resultado_reordenado);
    assert_eq!(cantidad(&resultado, ClaseConflicto::Type), 1);
    assert_eq!(cantidad(&resultado, ClaseConflicto::HasLab), 2);
    assert_eq!(cantidad(&resultado, ClaseConflicto::Parent), 1);
    assert_eq!(resultado.total_conflictos(), 4);
    assert_eq!(
        resultado
            .diagnosticos()
            .iter()
            .map(|diagnostico| (diagnostico.clase(), diagnostico.cantidad()))
            .collect::<Vec<_>>(),
        [
            (ClaseConflicto::Type, 1),
            (ClaseConflicto::HasLab, 2),
            (ClaseConflicto::Parent, 1)
        ]
    );
    let salida = format!("{resultado} {resultado:?}");
    for canario in [
        "PRIVATE_ORIGIN",
        "PRIVATE_DEST",
        "PRIVATE_PARENT",
        "PRIVATE_NODE_LABEL",
        "PRIVATE_EDGE_LABEL",
    ] {
        assert!(!salida.contains(canario), "se filtró {canario}: {salida}");
    }
    assert_eq!(snapshot.atomos(), antes);
}

#[test]
fn topologia_edge_invalida_sigue_devolviendo_error_redactado() {
    let mut snapshot = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    nodo(&mut snapshot, "secreto/PRIVATE_ORIGIN", "a", "origen");
    nodo(&mut snapshot, "secreto/PRIVATE_DEST", "b", "destino");
    let delta = Delta::nuevo(
        [],
        [Atomo::Edge {
            id: id_arista("secreto/PRIVATE_ORIGIN", "secreto/PRIVATE_DEST", "rel", 0),
            origen: p("secreto/PRIVATE_ORIGIN"),
            destino: p("secreto/PRIVATE_DEST"),
            sorte: Sorte::nuevo("rel"),
        }],
    )
    .unwrap();

    let error = detectar_conflictos_funcionales(&snapshot, &delta).unwrap_err();

    assert!(matches!(
        error,
        ErrorCandidato::RemoveNotFound {
            clase: ClaseOperacion::Edge
        }
    ));
    let salida = format!("{error} {error:?}");
    for canario in ["PRIVATE_ORIGIN", "PRIVATE_DEST"] {
        assert!(!salida.contains(canario), "se filtró {canario}: {salida}");
    }
}
