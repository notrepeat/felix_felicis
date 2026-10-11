use universo_core::{
    Atomo, BundlePath, ClaseOperacion, Delta, ErrorCandidato, Estructura, Etiqueta, Sorte, Sujeto,
    construir_candidato,
};

fn p(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn base() -> (
    Estructura,
    universo_core::NodoId,
    universo_core::NodoId,
    universo_core::NodoId,
) {
    let mut estructura = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let raiz = estructura
        .insertar_nodo(p("n/raiz"), Sorte::nuevo("a"), Etiqueta::nuevo("raiz"))
        .unwrap();
    let hijo = estructura
        .insertar_nodo(p("n/hijo"), Sorte::nuevo("b"), Etiqueta::nuevo("hijo"))
        .unwrap();
    let otro = estructura
        .insertar_nodo(p("n/otro"), Sorte::nuevo("b"), Etiqueta::nuevo("otro"))
        .unwrap();
    estructura.asignar_padre(hijo, raiz).unwrap();
    (estructura, raiz, hijo, otro)
}

fn haslab(sujeto: &str, etiqueta: &str) -> Atomo {
    Atomo::HasLab {
        sujeto: Sujeto::Nodo(p(sujeto)),
        etiqueta: Etiqueta::nuevo(etiqueta),
    }
}

fn parent(hijo: &str, padre: &str) -> Atomo {
    Atomo::Parent {
        hijo: p(hijo),
        padre: p(padre),
    }
}

fn assert_redactado(error: &ErrorCandidato, canarios: &[&str]) {
    let texto = format!("{error} {error:?}");
    for canario in canarios {
        assert!(!texto.contains(canario), "se filtró {canario}: {texto}");
    }
}

#[test]
fn haslab_nodo_identico_es_noop_y_preserva_snapshot() {
    let (snapshot, _, hijo, _) = base();
    let antes = snapshot.atomos();
    let delta = Delta::nuevo([haslab("n/hijo", "hijo")], []).unwrap();

    let candidato = construir_candidato(&snapshot, &delta).unwrap();

    assert_eq!(candidato.estructura().atomos(), antes);
    assert_eq!(snapshot.atomos(), antes);
    assert_eq!(
        candidato
            .estructura()
            .nodo(hijo)
            .unwrap()
            .etiqueta
            .como_str(),
        "hijo"
    );
    candidato.estructura().verificar_representacion().unwrap();
}

#[test]
fn haslab_nodo_reemplazo_exige_retiro_exacto_y_cambia_solo_el_candidato() {
    let (snapshot, _, hijo, _) = base();
    let antes = snapshot.atomos();
    let incompleto = Delta::nuevo([haslab("n/hijo", "nuevo")], []).unwrap();
    assert!(matches!(
        construir_candidato(&snapshot, &incompleto),
        Err(ErrorCandidato::AddConflict {
            clase: ClaseOperacion::HasLab
        })
    ));

    let reemplazo = Delta::nuevo([haslab("n/hijo", "nuevo")], [haslab("n/hijo", "hijo")]).unwrap();
    let candidato = construir_candidato(&snapshot, &reemplazo).unwrap();

    assert_eq!(
        candidato
            .estructura()
            .nodo(hijo)
            .unwrap()
            .etiqueta
            .como_str(),
        "nuevo"
    );
    assert_eq!(snapshot.atomos(), antes);
    candidato.estructura().verificar_representacion().unwrap();
}

#[test]
fn haslab_nodo_no_admite_retiro_aislado_ni_retiro_inexacto() {
    let (snapshot, _, _, _) = base();
    let antes = snapshot.atomos();
    let retiros = [
        (haslab("n/hijo", "hijo"), true),
        (haslab("n/hijo", "PRIV_LABEL"), false),
        (haslab("n/PRIV_NODE", "hijo"), false),
    ];
    for (retiro, add_conflict) in retiros {
        let delta = Delta::nuevo([], [retiro]).unwrap();
        let error = construir_candidato(&snapshot, &delta).unwrap_err();
        if add_conflict {
            assert!(matches!(
                &error,
                ErrorCandidato::AddConflict {
                    clase: ClaseOperacion::HasLab
                }
            ));
        } else {
            assert!(matches!(
                &error,
                ErrorCandidato::RemoveNotFound {
                    clase: ClaseOperacion::HasLab
                }
            ));
        }
        assert_redactado(&error, &["PRIV_LABEL", "PRIV_NODE"]);
        assert_eq!(snapshot.atomos(), antes);
    }
    let alta_ausente = Delta::nuevo([haslab("n/PRIV_NODE", "label")], []).unwrap();
    let error = construir_candidato(&snapshot, &alta_ausente).unwrap_err();
    assert!(matches!(
        error,
        ErrorCandidato::AddConflict {
            clase: ClaseOperacion::HasLab
        }
    ));
    assert_redactado(&error, &["PRIV_NODE", "label"]);
    assert_eq!(snapshot.atomos(), antes);
}

#[test]
fn haslab_nodo_rechaza_cr_y_lf_redactado_sin_mutar_entradas() {
    let (snapshot, _, _, _) = base();
    let antes = snapshot.atomos();

    for etiqueta in ["PRIV_LF\ncontinúa", "PRIV_CR\rcontinúa"] {
        let delta = Delta::nuevo([haslab("n/hijo", etiqueta)], [haslab("n/hijo", "hijo")]).unwrap();
        let agregar_antes = delta.agregar().to_vec();
        let retirar_antes = delta.retirar().to_vec();
        let error = construir_candidato(&snapshot, &delta).unwrap_err();

        assert!(matches!(
            &error,
            ErrorCandidato::AddConflict {
                clase: ClaseOperacion::HasLab
            }
        ));
        assert_redactado(&error, &[etiqueta, "PRIV_LF", "PRIV_CR"]);
        assert_eq!(delta.agregar(), agregar_antes.as_slice());
        assert_eq!(delta.retirar(), retirar_antes.as_slice());
        assert_eq!(snapshot.atomos(), antes);
    }
}

#[test]
fn haslab_conflictivo_redactado_y_misma_etiqueta_en_sujetos_distintos_valida() {
    let (snapshot, _, hijo, otro) = base();
    let antes = snapshot.atomos();
    let multiples = Delta::nuevo([haslab("n/hijo", "uno"), haslab("n/hijo", "dos")], []).unwrap();
    let error = construir_candidato(&snapshot, &multiples).unwrap_err();
    assert!(matches!(
        error,
        ErrorCandidato::AddConflict {
            clase: ClaseOperacion::HasLab
        }
    ));
    assert_redactado(&error, &["n/hijo", "uno", "dos"]);
    assert_eq!(snapshot.atomos(), antes);

    let misma = Delta::nuevo(
        [
            haslab("n/hijo", "compartida"),
            haslab("n/otro", "compartida"),
        ],
        [haslab("n/hijo", "hijo"), haslab("n/otro", "otro")],
    )
    .unwrap();
    let candidato = construir_candidato(&snapshot, &misma).unwrap();
    assert_eq!(
        candidato
            .estructura()
            .nodo(hijo)
            .unwrap()
            .etiqueta
            .como_str(),
        "compartida"
    );
    assert_eq!(
        candidato
            .estructura()
            .nodo(otro)
            .unwrap()
            .etiqueta
            .como_str(),
        "compartida"
    );
    assert_eq!(snapshot.atomos(), antes);
}

#[test]
fn parent_add_a_raiz_y_add_identico_son_validos() {
    let (snapshot, raiz, hijo, otro) = base();
    let antes = snapshot.atomos();
    let delta = Delta::nuevo([parent("n/otro", "n/raiz"), parent("n/hijo", "n/raiz")], []).unwrap();

    let candidato = construir_candidato(&snapshot, &delta).unwrap();
    let salida = candidato.estructura();

    assert_eq!(salida.padre(otro), Some(raiz));
    assert_eq!(salida.padre(hijo), Some(raiz));
    assert_eq!(snapshot.padre(otro), None);
    assert_eq!(snapshot.atomos(), antes);
    salida.verificar_representacion().unwrap();
}

#[test]
fn parent_remove_exacto_deja_raiz() {
    let (snapshot, raiz, hijo, _) = base();
    let antes = snapshot.atomos();
    let delta = Delta::nuevo([], [parent("n/hijo", "n/raiz")]).unwrap();

    let candidato = construir_candidato(&snapshot, &delta).unwrap();

    assert_eq!(candidato.estructura().padre(hijo), None);
    assert!(!candidato.estructura().hijos(raiz).contains(&hijo));
    assert_eq!(snapshot.padre(hijo), Some(raiz));
    assert_eq!(snapshot.atomos(), antes);
    candidato.estructura().verificar_representacion().unwrap();
}

#[test]
fn parent_reemplazo_exige_retiro_exacto_y_aplica_remove_add() {
    let (snapshot, _, hijo, otro) = base();
    let antes = snapshot.atomos();
    let incompleto = Delta::nuevo([parent("n/hijo", "n/otro")], []).unwrap();
    let error = construir_candidato(&snapshot, &incompleto).unwrap_err();
    assert!(matches!(
        error,
        ErrorCandidato::AddConflict {
            clase: ClaseOperacion::Parent
        }
    ));
    assert_redactado(&error, &["n/hijo", "n/otro"]);

    let reemplazo =
        Delta::nuevo([parent("n/hijo", "n/otro")], [parent("n/hijo", "n/raiz")]).unwrap();
    let candidato = construir_candidato(&snapshot, &reemplazo).unwrap();

    assert_eq!(candidato.estructura().padre(hijo), Some(otro));
    assert_eq!(snapshot.atomos(), antes);
    candidato.estructura().verificar_representacion().unwrap();
}

#[test]
fn parent_remove_inexacto_o_multiple_es_rechazado_redactado() {
    let (snapshot, _, _, _) = base();
    let antes = snapshot.atomos();
    let casos = [
        Delta::nuevo([], [parent("n/hijo", "n/otro")]).unwrap(),
        Delta::nuevo([], [parent("n/hijo", "n/raiz"), parent("n/hijo", "n/otro")]).unwrap(),
    ];
    for delta in casos {
        let error = construir_candidato(&snapshot, &delta).unwrap_err();
        assert!(matches!(
            error,
            ErrorCandidato::RemoveNotFound {
                clase: ClaseOperacion::Parent
            }
        ));
        assert_redactado(&error, &["n/hijo", "n/raiz", "n/otro"]);
        assert_eq!(snapshot.atomos(), antes);
    }
}

#[test]
fn parent_rechaza_conflictos_estructurales_redactados() {
    let (snapshot, _, _, _) = base();
    let antes = snapshot.atomos();
    let casos = [
        (
            Delta::nuevo([parent("n/raiz", "n/raiz")], []).unwrap(),
            "n/raiz",
            true,
        ),
        (
            Delta::nuevo([parent("n/raiz", "n/hijo")], []).unwrap(),
            "n/hijo",
            true,
        ),
        (
            Delta::nuevo([parent("n/PRIV_CHILD", "n/raiz")], []).unwrap(),
            "PRIV_CHILD",
            true,
        ),
        (
            Delta::nuevo([parent("n/otro", "n/PRIV_PARENT")], []).unwrap(),
            "PRIV_PARENT",
            true,
        ),
        (
            Delta::nuevo([parent("n/otro", "n/raiz"), parent("n/otro", "n/hijo")], []).unwrap(),
            "n/otro",
            true,
        ),
        (
            Delta::nuevo([], [parent("n/PRIV_CHILD", "n/raiz")]).unwrap(),
            "PRIV_CHILD",
            false,
        ),
        (
            Delta::nuevo([], [parent("n/otro", "n/PRIV_PARENT")]).unwrap(),
            "PRIV_PARENT",
            false,
        ),
    ];

    for (delta, canario, add_conflict) in casos {
        let error = construir_candidato(&snapshot, &delta).unwrap_err();
        if add_conflict {
            assert!(matches!(
                &error,
                ErrorCandidato::AddConflict {
                    clase: ClaseOperacion::Parent
                }
            ));
        } else {
            assert!(matches!(
                &error,
                ErrorCandidato::RemoveNotFound {
                    clase: ClaseOperacion::Parent
                }
            ));
        }
        assert_redactado(&error, &[canario]);
        assert_eq!(snapshot.atomos(), antes);
    }
}

#[test]
fn parent_resuelve_todos_los_retiros_antes_de_reasignar() {
    let mut snapshot = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let a = snapshot
        .insertar_nodo(p("n/a"), Sorte::nuevo("a"), Etiqueta::nuevo("a"))
        .unwrap();
    let b = snapshot
        .insertar_nodo(p("n/b"), Sorte::nuevo("b"), Etiqueta::nuevo("b"))
        .unwrap();
    let c = snapshot
        .insertar_nodo(p("n/c"), Sorte::nuevo("b"), Etiqueta::nuevo("c"))
        .unwrap();
    let d = snapshot
        .insertar_nodo(p("n/d"), Sorte::nuevo("b"), Etiqueta::nuevo("d"))
        .unwrap();
    snapshot.asignar_padre(a, b).unwrap();
    snapshot.asignar_padre(b, c).unwrap();
    let antes = snapshot.atomos();
    let delta = Delta::nuevo(
        [parent("n/c", "n/a"), parent("n/b", "n/d")],
        [parent("n/b", "n/c")],
    )
    .unwrap();

    let candidato = construir_candidato(&snapshot, &delta).unwrap();

    assert_eq!(candidato.estructura().padre(c), Some(a));
    assert_eq!(candidato.estructura().padre(b), Some(d));
    assert_eq!(snapshot.atomos(), antes);
    candidato.estructura().verificar_representacion().unwrap();
}

#[test]
fn parent_retiros_antes_de_altas_evitan_ciclo_transitorio() {
    let mut snapshot = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let a = snapshot
        .insertar_nodo(p("n/a"), Sorte::nuevo("a"), Etiqueta::nuevo("a"))
        .unwrap();
    let b = snapshot
        .insertar_nodo(p("n/b"), Sorte::nuevo("b"), Etiqueta::nuevo("b"))
        .unwrap();
    let c = snapshot
        .insertar_nodo(p("n/c"), Sorte::nuevo("b"), Etiqueta::nuevo("c"))
        .unwrap();
    snapshot.asignar_padre(a, b).unwrap();
    snapshot.asignar_padre(b, c).unwrap();
    let antes = snapshot.atomos();
    let delta = Delta::nuevo([parent("n/c", "n/a")], [parent("n/b", "n/c")]).unwrap();

    // La alta c -> a crearía a -> b -> c -> a hasta que se retire b -> c.
    let candidato = construir_candidato(&snapshot, &delta).unwrap();

    assert_eq!(candidato.estructura().padre(a), Some(b));
    assert_eq!(candidato.estructura().padre(b), None);
    assert_eq!(candidato.estructura().padre(c), Some(a));
    assert_eq!(snapshot.atomos(), antes);
    candidato.estructura().verificar_representacion().unwrap();
}

#[test]
fn haslab_parent_edge_y_ext_se_componen_en_un_candidato_aislado() {
    let (snapshot, raiz, hijo, otro) = base();
    let antes = snapshot.atomos();
    let origen = p("n/raiz");
    let destino = p("n/otro");
    let sorte = Sorte::nuevo("rel");
    let id = universo_core::IdArista {
        origen: origen.clone(),
        sorte: sorte.clone(),
        destino: destino.clone(),
        ordinal: 0,
    };
    let delta = Delta::nuevo(
        [
            Atomo::Edge {
                id: id.clone(),
                origen,
                destino,
                sorte,
            },
            Atomo::HasLab {
                sujeto: Sujeto::Arista(id),
                etiqueta: Etiqueta::nuevo("relación"),
            },
            haslab("n/hijo", "hijo actualizado"),
            parent("n/otro", "n/raiz"),
            Atomo::Ext {
                predicado: "Nivel".into(),
                sujeto: p("n/hijo"),
                constante: "alto".into(),
            },
        ],
        [haslab("n/hijo", "hijo")],
    )
    .unwrap();

    let candidato = construir_candidato(&snapshot, &delta).unwrap();
    let salida = candidato.estructura();

    assert_eq!(salida.len_aristas(), 1);
    assert_eq!(salida.padre(otro), Some(raiz));
    assert_eq!(
        salida.nodo(hijo).unwrap().etiqueta.como_str(),
        "hijo actualizado"
    );
    assert_eq!(
        salida.atributos(hijo).get("Nivel").map(String::as_str),
        Some("alto")
    );
    assert_eq!(snapshot.atomos(), antes);
    assert_eq!(snapshot.len_aristas(), 0);
    salida.verificar_representacion().unwrap();
}
