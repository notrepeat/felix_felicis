use universo_core::sexpr::parsear_programa;
use universo_core::{
    Atomo, BundlePath, ClaseConflicto, Delta, ErrorPropuesta, Estructura, EtapaErrorPropuesta,
    Etiqueta, IdArista, ResultadoPropuesta, Sorte, Sujeto, proponer,
};

fn p(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn nodo(estructura: &mut Estructura, path: &str, tipo: &str, etiqueta: &str) {
    estructura
        .insertar_nodo(p(path), Sorte::nuevo(tipo), Etiqueta::nuevo(etiqueta))
        .unwrap();
}

fn base() -> Estructura {
    let mut estructura = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    nodo(&mut estructura, "n/PRIVATE_A", "a", "PRIVATE_LABEL_A");
    nodo(&mut estructura, "n/PRIVATE_B", "b", "PRIVATE_LABEL_B");
    nodo(&mut estructura, "n/PRIVATE_C", "b", "PRIVATE_LABEL_C");
    let hijo = estructura.por_path(&p("n/PRIVATE_B")).unwrap();
    let padre = estructura.por_path(&p("n/PRIVATE_A")).unwrap();
    estructura.asignar_padre(hijo, padre).unwrap();
    estructura
}

fn delta(atomos: impl IntoIterator<Item = Atomo>) -> Delta {
    Delta::nuevo(atomos, []).unwrap()
}

fn edge(id: &IdArista, etiqueta: &str) -> [Atomo; 2] {
    [
        Atomo::Edge {
            id: id.clone(),
            origen: id.origen.clone(),
            destino: id.destino.clone(),
            sorte: id.sorte.clone(),
        },
        Atomo::HasLab {
            sujeto: Sujeto::Arista(id.clone()),
            etiqueta: Etiqueta::nuevo(etiqueta),
        },
    ]
}

fn fuentes(texto: &str) -> Vec<universo_core::SentenciaFuente> {
    parsear_programa(texto).unwrap()
}

fn assert_conflicto(resultado: ResultadoPropuesta, clase: ClaseConflicto) {
    let ResultadoPropuesta::RechazadaConflictos(conflictos) = resultado else {
        panic!("se esperaba rechazo funcional, salió {resultado:?}");
    };
    assert!(
        conflictos
            .diagnosticos()
            .iter()
            .any(|diagnostico| diagnostico.clase() == clase)
    );
}

#[test]
fn admite_delta_vacio_y_sentencia_verdadera_sin_mutar_snapshot() {
    let snapshot = base();
    let antes = snapshot.atomos();
    let vacio = Delta::nuevo([], []).unwrap();

    let resultado = proponer(&snapshot, &vacio, &[]).unwrap();
    let ResultadoPropuesta::Admitida(candidato) = resultado else {
        panic!("el delta vacío debe admitirse: {resultado:?}");
    };
    assert_eq!(candidato.estructura().atomos(), antes);

    let resultado = proponer(&snapshot, &vacio, &fuentes("(Type @n/PRIVATE_A a)")).unwrap();
    assert!(matches!(resultado, ResultadoPropuesta::Admitida(_)));
    assert_eq!(snapshot.atomos(), antes);
}

#[test]
fn rechaza_conflictos_type_haslab_y_parent_con_diagnosticos_redactados() {
    let mut snapshot = base();
    let origen = snapshot.por_path(&p("n/PRIVATE_A")).unwrap();
    let destino = snapshot.por_path(&p("n/PRIVATE_B")).unwrap();
    snapshot
        .insertar_arista(
            origen,
            destino,
            Sorte::nuevo("rel"),
            Etiqueta::nuevo("PRIVATE_EDGE_OLD"),
        )
        .unwrap();
    let antes = snapshot.atomos();
    let path_a = p("n/PRIVATE_A");
    let path_b = p("n/PRIVATE_B");
    let path_c = p("n/PRIVATE_C");
    let casos = [
        (
            delta([Atomo::HasLab {
                sujeto: Sujeto::Nodo(path_a.clone()),
                etiqueta: Etiqueta::nuevo("PRIVATE_LABEL_NEW"),
            }]),
            ClaseConflicto::HasLab,
            &["PRIVATE_A", "PRIVATE_LABEL_NEW"][..],
        ),
        (
            delta(edge(
                &IdArista {
                    origen: path_a.clone(),
                    sorte: Sorte::nuevo("rel"),
                    destino: path_b.clone(),
                    ordinal: 0,
                },
                "PRIVATE_EDGE_NEW",
            )),
            ClaseConflicto::HasLab,
            &[
                "PRIVATE_A",
                "PRIVATE_B",
                "PRIVATE_EDGE_OLD",
                "PRIVATE_EDGE_NEW",
            ][..],
        ),
        (
            delta([Atomo::Parent {
                hijo: path_b,
                padre: path_c,
            }]),
            ClaseConflicto::Parent,
            &["PRIVATE_B", "PRIVATE_C"][..],
        ),
    ];

    for (delta, clase, canarios) in casos {
        let resultado = proponer(&snapshot, &delta, &[]).unwrap();
        let salida = format!("{resultado} {resultado:?}");
        for canario in canarios {
            assert!(!salida.contains(canario), "se filtró {canario}: {salida}");
        }
        assert_conflicto(resultado, clase);
        assert_eq!(snapshot.atomos(), antes);
    }
}

#[test]
fn proponer_agrega_conflictos_edge_haslab_y_parent_en_orden_redactado() {
    let mut snapshot = base();
    let origen = snapshot.por_path(&p("n/PRIVATE_A")).unwrap();
    let destino = snapshot.por_path(&p("n/PRIVATE_B")).unwrap();
    snapshot
        .insertar_arista(
            origen,
            destino,
            Sorte::nuevo("rel"),
            Etiqueta::nuevo("PRIVATE_EDGE_OLD"),
        )
        .unwrap();
    let antes = snapshot.atomos();
    let id_edge = IdArista {
        origen: p("n/PRIVATE_A"),
        sorte: Sorte::nuevo("rel"),
        destino: p("n/PRIVATE_B"),
        ordinal: 0,
    };
    let delta = Delta::nuevo(
        [
            Atomo::HasLab {
                sujeto: Sujeto::Nodo(p("n/PRIVATE_A")),
                etiqueta: Etiqueta::nuevo("PRIVATE_NODE_LABEL_NEW"),
            },
            Atomo::Parent {
                hijo: p("n/PRIVATE_B"),
                padre: p("n/PRIVATE_C"),
            },
            edge(&id_edge, "PRIVATE_EDGE_NEW")[0].clone(),
            edge(&id_edge, "PRIVATE_EDGE_NEW")[1].clone(),
        ],
        [],
    )
    .unwrap();

    let primera = proponer(&snapshot, &delta, &[]).unwrap();
    let segunda = proponer(&snapshot, &delta, &[]).unwrap();
    let salida = format!("{primera} {primera:?}");
    assert_eq!(format!("{primera:?}"), format!("{segunda:?}"));
    assert_eq!(primera.to_string(), segunda.to_string());
    for canario in [
        "PRIVATE_A",
        "PRIVATE_B",
        "PRIVATE_C",
        "PRIVATE_EDGE_OLD",
        "PRIVATE_EDGE_NEW",
        "PRIVATE_NODE_LABEL_NEW",
        "candidate_error",
        "add_conflict",
        "remove_not_found",
    ] {
        assert!(!salida.contains(canario), "se filtró {canario}: {salida}");
    }
    let ResultadoPropuesta::RechazadaConflictos(conflictos) = primera else {
        panic!("se esperaba rechazo funcional agregado: {primera:?}");
    };
    let diagnosticos = conflictos.diagnosticos();
    assert_eq!(diagnosticos.len(), 2);
    assert_eq!(diagnosticos[0].clase(), ClaseConflicto::HasLab);
    assert_eq!(diagnosticos[0].cantidad(), 2);
    assert_eq!(diagnosticos[1].clase(), ClaseConflicto::Parent);
    assert_eq!(diagnosticos[1].cantidad(), 1);
    assert_eq!(conflictos.total_conflictos(), 3);
    assert_eq!(snapshot.atomos(), antes);
}

#[test]
fn cualquier_delta_type_es_tecnico_aunque_el_detector_vea_conflicto() {
    let snapshot = base();
    let antes = snapshot.atomos();
    let casos = [
        Atomo::Type {
            sujeto: p("n/PRIVATE_A"),
            sorte: Sorte::nuevo("b"),
        },
        Atomo::Type {
            sujeto: p("n/PRIVATE_NEW"),
            sorte: Sorte::nuevo("PRIVATE_TYPE"),
        },
    ];

    for atomo in casos {
        let error = proponer(&snapshot, &delta([atomo]), &[]).unwrap_err();
        assert_eq!(error.etapa(), EtapaErrorPropuesta::Candidato);
        assert_eq!(error.codigo(), "unsupported_operation");
        assert_eq!(error.clase(), Some(universo_core::ClaseOperacion::Type));
        assert_redactado(&error, &["PRIVATE_A", "PRIVATE_NEW", "PRIVATE_TYPE"]);
        assert_eq!(snapshot.atomos(), antes);
    }
}

#[test]
fn retiros_invalidos_edge_invalido_haslab_ausente_huerfano_y_ciclo_siguen_tecnicos() {
    let snapshot = base();
    let casos = [
        (
            Delta::nuevo(
                [],
                [Atomo::Parent {
                    hijo: p("n/PRIVATE_A"),
                    padre: p("n/PRIVATE_C"),
                }],
            )
            .unwrap(),
            "remove_not_found",
            universo_core::ClaseOperacion::Parent,
        ),
        (
            delta([Atomo::Edge {
                id: IdArista {
                    origen: p("n/PRIVATE_MISSING"),
                    sorte: Sorte::nuevo("rel"),
                    destino: p("n/PRIVATE_B"),
                    ordinal: 0,
                },
                origen: p("n/PRIVATE_MISSING"),
                destino: p("n/PRIVATE_B"),
                sorte: Sorte::nuevo("rel"),
            }]),
            "add_conflict",
            universo_core::ClaseOperacion::Edge,
        ),
        (
            delta([Atomo::Edge {
                id: IdArista {
                    origen: p("n/PRIVATE_A"),
                    sorte: Sorte::nuevo("rel"),
                    destino: p("n/PRIVATE_C"),
                    ordinal: 0,
                },
                origen: p("n/PRIVATE_A"),
                destino: p("n/PRIVATE_C"),
                sorte: Sorte::nuevo("rel"),
            }]),
            "label_missing",
            universo_core::ClaseOperacion::HasLab,
        ),
        (
            delta([Atomo::HasLab {
                sujeto: Sujeto::Arista(IdArista {
                    origen: p("n/PRIVATE_A"),
                    sorte: Sorte::nuevo("rel"),
                    destino: p("n/PRIVATE_C"),
                    ordinal: 7,
                }),
                etiqueta: Etiqueta::nuevo("PRIVATE_ORPHAN_LABEL"),
            }]),
            "label_orphan",
            universo_core::ClaseOperacion::HasLab,
        ),
        (
            delta([Atomo::Parent {
                hijo: p("n/PRIVATE_A"),
                padre: p("n/PRIVATE_B"),
            }]),
            "parent_cycle",
            universo_core::ClaseOperacion::Parent,
        ),
    ];

    for (delta, codigo, clase) in casos {
        let antes = snapshot.atomos();
        let error = proponer(&snapshot, &delta, &[]).unwrap_err();
        assert_eq!(error.etapa(), EtapaErrorPropuesta::Candidato);
        assert_eq!(error.codigo(), codigo);
        assert_eq!(error.clase(), Some(clase));
        assert_redactado(
            &error,
            &["PRIVATE_MISSING", "PRIVATE_ORPHAN_LABEL", "PRIVATE_C"],
        );
        assert_eq!(snapshot.atomos(), antes);
    }
}

#[test]
fn invariantes_se_evalúan_sobre_candidato_edge_y_ext_y_solo_exponen_procedencia() {
    let snapshot = base();
    let antes = snapshot.atomos();
    let id = IdArista {
        origen: p("n/PRIVATE_A"),
        sorte: Sorte::nuevo("rel"),
        destino: p("n/PRIVATE_B"),
        ordinal: 0,
    };
    let edge_delta = delta(edge(&id, "PRIVATE_EDGE_LABEL"));
    let casos = [
        (
            edge_delta,
            "; PRIVATE_EDGE_COMMENT_CANARY\n(not (exists ((e E)) (Edge e @n/PRIVATE_A @n/PRIVATE_B rel)))",
            &[
                "PRIVATE_EDGE_LABEL",
                "PRIVATE_A",
                "PRIVATE_B",
                "PRIVATE_EDGE_COMMENT_CANARY",
                "(not (exists",
            ][..],
        ),
        (
            delta([Atomo::Ext {
                predicado: "Nivel".into(),
                sujeto: p("n/PRIVATE_A"),
                constante: "alto".into(),
            }]),
            "; cabecera privada\n(not (Nivel @n/PRIVATE_A alto))",
            &["PRIVATE_A"][..],
        ),
    ];

    for (delta, texto, canarios) in casos {
        let resultado = proponer(&snapshot, &delta, &fuentes(texto)).unwrap();
        let ResultadoPropuesta::RechazadaInvariantes(procedencias) = &resultado else {
            panic!("se esperaba rechazo por invariante: {resultado:?}");
        };
        assert_eq!(procedencias.len(), 1);
        assert_eq!(procedencias[0].source_index, 0);
        assert_eq!(procedencias[0].linea, Some(2));
        assert_eq!(procedencias[0].columna, Some(1));
        assert_eq!(procedencias[0].instance_index, None);
        let salida = format!("{resultado} {resultado:?}");
        for canario in canarios {
            assert!(!salida.contains(canario), "se filtró {canario}: {salida}");
        }
        assert!(!salida.contains("exists"));
        assert_eq!(snapshot.atomos(), antes);
    }
}

#[test]
fn invariantes_tecnicos_distinguen_expansion_evaluacion_y_candidato() {
    let snapshot = base();
    let antes = snapshot.atomos();
    let vacio = Delta::nuevo([], []).unwrap();
    let expansion = proponer(
        &snapshot,
        &vacio,
        &fuentes("; PRIVATE_COMMENT_CANARY\n(Type @n/PRIVATE_MISSING PRIVATE_FORMULA_CANARY)"),
    )
    .unwrap_err();
    assert_eq!(expansion.etapa(), EtapaErrorPropuesta::Expansion);
    assert_eq!(expansion.codigo(), "invalid_invariant_statement");
    assert_redactado(
        &expansion,
        &[
            "PRIVATE_MISSING",
            "PRIVATE_FORMULA_CANARY",
            "PRIVATE_COMMENT_CANARY",
            "(Type",
        ],
    );

    let candidato = proponer(
        &snapshot,
        &delta([Atomo::Ext {
            predicado: "Nivel".into(),
            sujeto: p("n/PRIVATE_A"),
            constante: "PRIVATE_VALUE".into(),
        }]),
        &[],
    )
    .unwrap_err();
    assert_eq!(candidato.etapa(), EtapaErrorPropuesta::Candidato);
    assert_eq!(candidato.codigo(), "add_conflict");
    assert_eq!(candidato.clase(), Some(universo_core::ClaseOperacion::Ext));
    assert_redactado(&candidato, &["PRIVATE_A", "PRIVATE_VALUE"]);
    assert_eq!(snapshot.atomos(), antes);
}

#[test]
fn agotamiento_de_evaluacion_es_error_tecnico_redactado() {
    let mut snapshot = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    for indice in 0..520 {
        nodo(&mut snapshot, &format!("n/nodo-{indice:04}"), "a", "label");
    }
    let fuentes = fuentes("(forall ((x N) (y N)) (or (= x y) (not (= x y))))");
    let error = proponer(&snapshot, &Delta::nuevo([], []).unwrap(), &fuentes).unwrap_err();

    assert_eq!(error.etapa(), EtapaErrorPropuesta::Evaluacion);
    assert_eq!(error.codigo(), "evaluation_budget_exhausted");
    assert_redactado(&error, &["nodo-0000", "(forall"]);
}

#[test]
fn error_de_evaluacion_no_expone_path_label_formula_ni_comentario() {
    let snapshot = base();
    let fuentes =
        fuentes("; PRIVATE_COMMENT_CANARY\n(HasLab @n/PRIVATE_A \"PRIVATE_LABEL_CANARY\")");
    let error = proponer(&snapshot, &Delta::nuevo([], []).unwrap(), &fuentes).unwrap_err();
    assert_eq!(error.etapa(), EtapaErrorPropuesta::Evaluacion);
    assert_eq!(error.codigo(), "label_unavailable");
    assert_redactado(
        &error,
        &[
            "PRIVATE_A",
            "PRIVATE_LABEL_CANARY",
            "PRIVATE_COMMENT_CANARY",
            "(HasLab",
        ],
    );
}

#[test]
fn etiquetas_iguales_en_sujetos_distintos_se_admiten_y_el_resultado_es_determinista() {
    let mut snapshot = base();
    let origen = snapshot.por_path(&p("n/PRIVATE_A")).unwrap();
    let destino = snapshot.por_path(&p("n/PRIVATE_B")).unwrap();
    snapshot
        .insertar_arista(
            origen,
            destino,
            Sorte::nuevo("rel"),
            Etiqueta::nuevo("PRIVATE_SHARED_LABEL"),
        )
        .unwrap();
    let nuevo = IdArista {
        origen: p("n/PRIVATE_A"),
        sorte: Sorte::nuevo("rel"),
        destino: p("n/PRIVATE_C"),
        ordinal: 0,
    };
    let delta = delta(edge(&nuevo, "PRIVATE_SHARED_LABEL"));
    let antes = snapshot.atomos();
    let primera = proponer(&snapshot, &delta, &[]).unwrap();
    let segunda = proponer(&snapshot, &delta, &[]).unwrap();

    assert!(matches!(&primera, ResultadoPropuesta::Admitida(_)));
    assert_eq!(format!("{primera:?}"), format!("{segunda:?}"));
    assert_eq!(primera.to_string(), segunda.to_string());
    let ResultadoPropuesta::Admitida(candidato) = primera else {
        unreachable!();
    };
    let ResultadoPropuesta::Admitida(otra) = segunda else {
        unreachable!();
    };
    assert_eq!(candidato.estructura().atomos(), otra.estructura().atomos());
    let estructura = candidato.estructura();
    let origen = estructura.por_path(&p("n/PRIVATE_A")).unwrap();
    let destino = estructura.por_path(&p("n/PRIVATE_C")).unwrap();
    let etiqueta = estructura
        .salientes(origen)
        .iter()
        .find_map(|handle| {
            let arista = estructura.arista(*handle)?;
            (arista.destino == destino).then_some(arista.etiqueta.como_str())
        })
        .unwrap();
    assert_eq!(etiqueta, "PRIVATE_SHARED_LABEL");
    assert_eq!(snapshot.atomos(), antes);
}

fn assert_redactado(error: &ErrorPropuesta, canarios: &[&str]) {
    let salida = format!("{error} {error:?}");
    for canario in canarios {
        assert!(!salida.contains(canario), "se filtró {canario}: {salida}");
    }
}
