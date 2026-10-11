use universo_core::{
    Atomo, BundlePath, ClaseOperacion, Delta, ErrorCandidato, Estructura, Etiqueta, IdArista,
    Sorte, Sujeto, construir_candidato,
};

fn p(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn paralelas(
    cantidad: u32,
) -> (
    Estructura,
    universo_core::NodoId,
    universo_core::NodoId,
    Vec<universo_core::AristaId>,
) {
    let mut estructura = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let origen = estructura
        .insertar_nodo(p("n/origen"), Sorte::nuevo("a"), Etiqueta::nuevo("origen"))
        .unwrap();
    let destino = estructura
        .insertar_nodo(
            p("n/destino"),
            Sorte::nuevo("b"),
            Etiqueta::nuevo("destino"),
        )
        .unwrap();
    let handles = (0..cantidad)
        .map(|ordinal| {
            estructura
                .insertar_arista(
                    origen,
                    destino,
                    Sorte::nuevo("rel"),
                    Etiqueta::nuevo(&format!("label-{ordinal}")),
                )
                .unwrap()
        })
        .collect();
    (estructura, origen, destino, handles)
}

fn edge(snapshot: &Estructura, ordinal: u32) -> Atomo {
    snapshot
        .atomos()
        .into_iter()
        .find(|atomo| matches!(atomo, Atomo::Edge { id, .. } if id.ordinal == ordinal))
        .unwrap()
}

fn identidad(snapshot: &Estructura, handle: universo_core::AristaId) -> IdArista {
    let arista = snapshot.arista(handle).unwrap();
    IdArista {
        origen: snapshot.nodo(arista.origen).unwrap().path.clone(),
        sorte: arista.tipo.clone(),
        destino: snapshot.nodo(arista.destino).unwrap().path.clone(),
        ordinal: arista.ordinal,
    }
}

fn edge_add(origen: &str, destino: &str, sorte: &str, ordinal: u32) -> (IdArista, Atomo) {
    let origen = p(origen);
    let destino = p(destino);
    let sorte = Sorte::nuevo(sorte);
    let id = IdArista {
        origen: origen.clone(),
        sorte: sorte.clone(),
        destino: destino.clone(),
        ordinal,
    };
    (
        id.clone(),
        Atomo::Edge {
            id,
            origen,
            destino,
            sorte,
        },
    )
}

fn haslab_add(id: IdArista, etiqueta: &str) -> Atomo {
    Atomo::HasLab {
        sujeto: Sujeto::Arista(id),
        etiqueta: Etiqueta::nuevo(etiqueta),
    }
}

#[test]
fn tres_paralelas_remove_cero_compacta_en_orden_y_conserva_labels_por_handle() {
    let (snapshot, origen, _, handles) = paralelas(3);
    let delta = Delta::nuevo([], [edge(&snapshot, 0)]).unwrap();
    let candidato = construir_candidato(&snapshot, &delta).unwrap();
    let estructura = candidato.estructura();

    assert_eq!(estructura.salientes(origen), &[handles[1], handles[2]]);
    assert_eq!(estructura.arista(handles[1]).unwrap().ordinal, 0);
    assert_eq!(
        estructura.arista(handles[1]).unwrap().etiqueta.como_str(),
        "label-1"
    );
    assert_eq!(estructura.arista(handles[2]).unwrap().ordinal, 1);
    assert_eq!(
        estructura.arista(handles[2]).unwrap().etiqueta.como_str(),
        "label-2"
    );
    assert!(snapshot.arista(handles[0]).is_some());
    estructura.verificar_representacion().unwrap();
}

#[test]
fn dos_retiros_paralelos_dejan_el_tercero_y_preservan_adyacencias_ajenas() {
    let (mut snapshot, origen, destino, handles) = paralelas(3);
    let otro_destino = snapshot
        .insertar_nodo(
            p("n/otro-destino"),
            Sorte::nuevo("b"),
            Etiqueta::nuevo("otro"),
        )
        .unwrap();
    let ajena = snapshot
        .insertar_arista(
            origen,
            otro_destino,
            Sorte::nuevo("rel"),
            Etiqueta::nuevo("grupo-ajeno"),
        )
        .unwrap();
    let delta = Delta::nuevo([], [edge(&snapshot, 0), edge(&snapshot, 1)]).unwrap();

    let candidato = construir_candidato(&snapshot, &delta).unwrap();
    let salida = candidato.estructura();

    assert_eq!(salida.salientes(origen), &[handles[2], ajena]);
    assert_eq!(salida.entrantes(destino), &[handles[2]]);
    assert_eq!(salida.entrantes(otro_destino), &[ajena]);
    assert_eq!(salida.arista(handles[2]).unwrap().ordinal, 0);
    assert_eq!(
        salida.arista(handles[2]).unwrap().etiqueta.como_str(),
        "label-2"
    );
    assert_eq!(salida.arista(ajena).unwrap().ordinal, 0);
    assert_eq!(
        salida.arista(ajena).unwrap().etiqueta.como_str(),
        "grupo-ajeno"
    );
    assert_eq!(
        snapshot.salientes(origen),
        &[handles[0], handles[1], handles[2], ajena]
    );
    assert_eq!(
        snapshot.entrantes(destino),
        &[handles[0], handles[1], handles[2]]
    );
    salida.verificar_representacion().unwrap();
}

#[test]
fn lote_reordena_retiros_de_forma_determinista_y_reasocia_label_al_handle() {
    let (snapshot, origen, _, handles) = paralelas(11);
    let uno = edge(&snapshot, 1);
    let diez = edge(&snapshot, 10);
    let a = Delta::nuevo([], [uno.clone(), diez.clone()]).unwrap();
    let b = Delta::nuevo([], [diez, uno]).unwrap();
    let candidato_a = construir_candidato(&snapshot, &a).unwrap();
    let candidato_b = construir_candidato(&snapshot, &b).unwrap();
    let salida_a = candidato_a.estructura();
    let salida_b = candidato_b.estructura();

    assert_eq!(salida_a.atomos(), salida_b.atomos());
    let esperados: Vec<_> = handles
        .iter()
        .copied()
        .filter(|handle| *handle != handles[1] && *handle != handles[10])
        .collect();
    assert_eq!(salida_a.salientes(origen), esperados);
    for (ordinal, handle) in esperados.iter().enumerate() {
        let superviviente = salida_a.arista(*handle).unwrap();
        assert_eq!(superviviente.ordinal, ordinal as u32);
        let original = if ordinal == 0 { 0 } else { ordinal + 1 };
        assert_eq!(
            superviviente.etiqueta.como_str(),
            format!("label-{original}")
        );
    }
    snapshot.verificar_representacion().unwrap();
    salida_a.verificar_representacion().unwrap();
    salida_b.verificar_representacion().unwrap();
}

#[test]
fn remove_valido_mas_inexistente_falla_sin_alterar_snapshot() {
    let (snapshot, _, _, _) = paralelas(3);
    let antes = snapshot.atomos();
    let inexistente = Atomo::parsear(
        "Edge(z/PRIVATE_FROM→z/PRIVATE_TO#rel#7, z/PRIVATE_FROM, z/PRIVATE_TO, rel)",
    )
    .unwrap();
    let delta = Delta::nuevo([], [edge(&snapshot, 0), inexistente]).unwrap();

    let error = construir_candidato(&snapshot, &delta).unwrap_err();
    assert!(matches!(
        error,
        ErrorCandidato::RemoveNotFound {
            clase: ClaseOperacion::Edge
        }
    ));
    let texto = format!("{error} {error:?}");
    assert!(texto.contains("remove_not_found"));
    assert!(!texto.contains("PRIVATE_FROM") && !texto.contains("PRIVATE_TO"));
    assert_eq!(snapshot.atomos(), antes);
    snapshot.verificar_representacion().unwrap();
}

#[test]
fn haslab_de_arista_exige_edge_del_mismo_handle_y_label_exacta() {
    let (snapshot, _, _, handles) = paralelas(2);
    let identidad = match edge(&snapshot, 0) {
        Atomo::Edge { id, .. } => id,
        _ => unreachable!(),
    };
    let etiqueta = snapshot.arista(handles[0]).unwrap().etiqueta.clone();
    let delta = Delta::nuevo(
        [],
        [
            edge(&snapshot, 0),
            Atomo::HasLab {
                sujeto: Sujeto::Arista(identidad),
                etiqueta: etiqueta.clone(),
            },
        ],
    )
    .unwrap();
    let salida = construir_candidato(&snapshot, &delta).unwrap();
    assert!(salida.estructura().arista(handles[0]).is_none());
    let superviviente = salida.estructura().arista(handles[1]).unwrap();
    assert_eq!(superviviente.ordinal, 0);
    assert_eq!(superviviente.etiqueta.como_str(), "label-1");

    let solo_label = Delta::nuevo(
        [],
        [Atomo::HasLab {
            sujeto: Sujeto::Arista(IdArista {
                origen: p("n/origen"),
                sorte: Sorte::nuevo("rel"),
                destino: p("n/destino"),
                ordinal: 0,
            }),
            etiqueta,
        }],
    )
    .unwrap();
    assert!(matches!(
        construir_candidato(&snapshot, &solo_label),
        Err(ErrorCandidato::AddConflict {
            clase: ClaseOperacion::HasLab,
        })
    ));

    let label_incorrecta = Delta::nuevo(
        [],
        [
            edge(&snapshot, 0),
            Atomo::HasLab {
                sujeto: Sujeto::Arista(IdArista {
                    origen: p("n/origen"),
                    sorte: Sorte::nuevo("rel"),
                    destino: p("n/destino"),
                    ordinal: 0,
                }),
                etiqueta: Etiqueta::nuevo("PRIVATE_LABEL"),
            },
        ],
    )
    .unwrap();
    let error = construir_candidato(&snapshot, &label_incorrecta).unwrap_err();
    assert!(matches!(
        error,
        ErrorCandidato::RemoveNotFound {
            clase: ClaseOperacion::HasLab
        }
    ));
    assert!(!format!("{error} {error:?}").contains("PRIVATE_LABEL"));
    snapshot.verificar_representacion().unwrap();
}

#[test]
fn once_altas_ordenan_ordinales_numericamente_y_asocian_labels_por_handle() {
    let (snapshot, origen, _, _) = paralelas(0);
    let antes = snapshot.atomos();
    let mut add = Vec::new();
    for ordinal in 0..11 {
        let (id, edge) = edge_add("n/origen", "n/destino", "rel", ordinal);
        add.push(edge);
        let etiqueta = if ordinal == 10 {
            "".to_string()
        } else {
            format!("nueva-{ordinal}")
        };
        add.push(haslab_add(id, &etiqueta));
    }
    let delta = Delta::nuevo(add.clone(), []).unwrap();
    let delta_inverso = Delta::nuevo(add.into_iter().rev(), []).unwrap();
    let salida = construir_candidato(&snapshot, &delta).unwrap();
    let salida_inversa = construir_candidato(&snapshot, &delta_inverso).unwrap();

    assert_eq!(
        salida.estructura().atomos(),
        salida_inversa.estructura().atomos()
    );
    let handles = salida.estructura().salientes(origen);
    assert_eq!(handles.len(), 11);
    for (ordinal, handle) in handles.iter().enumerate() {
        let arista = salida.estructura().arista(*handle).unwrap();
        assert_eq!(arista.ordinal, ordinal as u32);
        let etiqueta = if ordinal == 10 {
            "".to_string()
        } else {
            format!("nueva-{ordinal}")
        };
        assert_eq!(arista.etiqueta.como_str(), etiqueta);
    }
    assert_eq!(snapshot.atomos(), antes);
    snapshot.verificar_representacion().unwrap();
    salida.estructura().verificar_representacion().unwrap();
    salida_inversa
        .estructura()
        .verificar_representacion()
        .unwrap();
}

#[test]
fn remove_y_add_usan_ordinales_finales_y_conservan_labels_por_handle() {
    let (snapshot, origen, _, handles) = paralelas(3);
    let (id_nueva, edge_nueva) = edge_add("n/origen", "n/destino", "rel", 2);
    let delta = Delta::nuevo(
        [edge_nueva, haslab_add(id_nueva, "nueva")],
        [edge(&snapshot, 1)],
    )
    .unwrap();
    let salida = construir_candidato(&snapshot, &delta).unwrap();
    let orden = salida.estructura().salientes(origen);

    assert_eq!(orden.len(), 3);
    assert_eq!(&orden[..2], &[handles[0], handles[2]]);
    assert_eq!(salida.estructura().arista(handles[0]).unwrap().ordinal, 0);
    assert_eq!(
        salida
            .estructura()
            .arista(handles[0])
            .unwrap()
            .etiqueta
            .como_str(),
        "label-0"
    );
    assert_eq!(salida.estructura().arista(handles[2]).unwrap().ordinal, 1);
    assert_eq!(
        salida
            .estructura()
            .arista(handles[2])
            .unwrap()
            .etiqueta
            .como_str(),
        "label-2"
    );
    assert_eq!(salida.estructura().arista(orden[2]).unwrap().ordinal, 2);
    assert_eq!(
        salida
            .estructura()
            .arista(orden[2])
            .unwrap()
            .etiqueta
            .como_str(),
        "nueva"
    );
    assert_eq!(snapshot.arista(handles[1]).unwrap().ordinal, 1);
    salida.estructura().verificar_representacion().unwrap();
}

#[test]
fn alta_edge_exige_label_y_rechaza_huerfano_o_varios_labels_distintos() {
    let (snapshot, _, _, _) = paralelas(0);
    let antes = snapshot.atomos();
    let (id, edge) = edge_add("n/origen", "n/destino", "rel", 0);
    let sin_label = Delta::nuevo([edge.clone()], []).unwrap();
    assert!(matches!(
        construir_candidato(&snapshot, &sin_label),
        Err(ErrorCandidato::AddConflict {
            clase: ClaseOperacion::HasLab
        })
    ));

    let huerfano = Delta::nuevo([haslab_add(id.clone(), "huerfano")], []).unwrap();
    assert!(matches!(
        construir_candidato(&snapshot, &huerfano),
        Err(ErrorCandidato::AddConflict {
            clase: ClaseOperacion::HasLab
        })
    ));

    let conflictivo = Delta::nuevo(
        [
            edge,
            haslab_add(id.clone(), "label-a"),
            haslab_add(id, "label-b"),
        ],
        [],
    )
    .unwrap();
    assert!(matches!(
        construir_candidato(&snapshot, &conflictivo),
        Err(ErrorCandidato::AddConflict {
            clase: ClaseOperacion::HasLab
        })
    ));
    assert_eq!(snapshot.atomos(), antes);
    snapshot.verificar_representacion().unwrap();
}

#[test]
fn alta_edge_rechaza_huecos_endpoints_y_tipos_ausentes_sin_revelar_paths() {
    let (snapshot, _, _, _) = paralelas(0);
    let antes = snapshot.atomos();
    let (id_hueco, edge_hueco) = edge_add("n/origen", "n/destino", "rel", 1);
    let hueco = Delta::nuevo([edge_hueco, haslab_add(id_hueco, "label")], []).unwrap();
    assert!(matches!(
        construir_candidato(&snapshot, &hueco),
        Err(ErrorCandidato::AddConflict {
            clase: ClaseOperacion::Edge
        })
    ));

    let (id_ausente, edge_ausente) = edge_add("n/PRIVATE_ENDPOINT", "n/destino", "rel", 0);
    let endpoint_ausente =
        Delta::nuevo([edge_ausente, haslab_add(id_ausente, "PRIVATE_LABEL")], []).unwrap();
    let error = construir_candidato(&snapshot, &endpoint_ausente).unwrap_err();
    assert!(matches!(
        error,
        ErrorCandidato::AddConflict {
            clase: ClaseOperacion::Edge
        }
    ));
    let texto = format!("{error} {error:?}");
    assert!(!texto.contains("PRIVATE_ENDPOINT") && !texto.contains("PRIVATE_LABEL"));

    let (id_tipo_ausente, edge_tipo_ausente) =
        edge_add("n/origen", "n/destino", "PRIVATE_RELATION", 0);
    let tipo_ausente = Delta::nuevo(
        [edge_tipo_ausente, haslab_add(id_tipo_ausente, "label")],
        [],
    )
    .unwrap();
    let error = construir_candidato(&snapshot, &tipo_ausente).unwrap_err();
    assert!(matches!(
        error,
        ErrorCandidato::AddConflict {
            clase: ClaseOperacion::Edge
        }
    ));
    assert!(!format!("{error} {error:?}").contains("PRIVATE_RELATION"));
    assert_eq!(snapshot.atomos(), antes);
    snapshot.verificar_representacion().unwrap();
}

#[test]
fn alta_idempotente_requiere_label_vigente_exacta_y_delta_vacio_es_noop() {
    let (snapshot, _, _, _) = paralelas(2);
    let antes = snapshot.atomos();
    let vacio = Delta::nuevo([], []).unwrap();
    assert_eq!(
        construir_candidato(&snapshot, &vacio)
            .unwrap()
            .estructura()
            .atomos(),
        antes
    );

    let vigente = edge(&snapshot, 1);
    let id = match &vigente {
        Atomo::Edge { id, .. } => id.clone(),
        _ => unreachable!(),
    };
    let etiqueta = snapshot
        .arista(snapshot.salientes(snapshot.por_path(&p("n/origen")).unwrap())[1])
        .unwrap()
        .etiqueta
        .clone();
    let no_op = Delta::nuevo(
        [vigente.clone(), haslab_add(id.clone(), etiqueta.como_str())],
        [],
    )
    .unwrap();
    assert_eq!(
        construir_candidato(&snapshot, &no_op)
            .unwrap()
            .estructura()
            .atomos(),
        antes
    );

    let label_distinta = Delta::nuevo([vigente, haslab_add(id, "PRIVATE_LABEL")], []).unwrap();
    let error = construir_candidato(&snapshot, &label_distinta).unwrap_err();
    assert!(matches!(
        error,
        ErrorCandidato::AddConflict {
            clase: ClaseOperacion::HasLab
        }
    ));
    assert!(!format!("{error} {error:?}").contains("PRIVATE_LABEL"));
    assert_eq!(snapshot.atomos(), antes);
}

#[test]
fn reemplaza_label_de_arista_superviviente_sin_cambiar_handle() {
    let (snapshot, _, _, handles) = paralelas(2);
    let id = identidad(&snapshot, handles[1]);
    let delta = Delta::nuevo(
        [haslab_add(id.clone(), "actualizada")],
        [Atomo::HasLab {
            sujeto: Sujeto::Arista(id),
            etiqueta: Etiqueta::nuevo("label-1"),
        }],
    )
    .unwrap();
    let antes = snapshot.atomos();
    let salida = construir_candidato(&snapshot, &delta).unwrap();

    assert_eq!(salida.estructura().arista(handles[1]).unwrap().ordinal, 1);
    assert_eq!(
        salida
            .estructura()
            .arista(handles[1])
            .unwrap()
            .etiqueta
            .como_str(),
        "actualizada"
    );
    assert_eq!(snapshot.atomos(), antes);
    salida.estructura().verificar_representacion().unwrap();
    snapshot.verificar_representacion().unwrap();
}

#[test]
fn reemplazo_de_label_usa_identidad_final_del_handle_tras_compactar() {
    let (snapshot, origen, _, handles) = paralelas(3);
    let id_snapshot = identidad(&snapshot, handles[2]);
    let id_final = IdArista {
        ordinal: 1,
        ..id_snapshot.clone()
    };
    let delta = Delta::nuevo(
        [haslab_add(id_final, "rebasada")],
        [
            edge(&snapshot, 0),
            Atomo::HasLab {
                sujeto: Sujeto::Arista(id_snapshot),
                etiqueta: Etiqueta::nuevo("label-2"),
            },
        ],
    )
    .unwrap();
    let antes = snapshot.atomos();
    let salida = construir_candidato(&snapshot, &delta).unwrap();
    let orden = salida.estructura().salientes(origen);

    assert_eq!(orden, &[handles[1], handles[2]]);
    assert_eq!(salida.estructura().arista(handles[1]).unwrap().ordinal, 0);
    assert_eq!(
        salida
            .estructura()
            .arista(handles[1])
            .unwrap()
            .etiqueta
            .como_str(),
        "label-1"
    );
    assert_eq!(salida.estructura().arista(handles[2]).unwrap().ordinal, 1);
    assert_eq!(
        salida
            .estructura()
            .arista(handles[2])
            .unwrap()
            .etiqueta
            .como_str(),
        "rebasada"
    );
    assert_eq!(snapshot.atomos(), antes);
    salida.estructura().verificar_representacion().unwrap();
    snapshot.verificar_representacion().unwrap();
}

#[test]
fn label_distinta_sin_remove_y_remove_sin_add_rechazan_sin_mutar_ni_filtrar() {
    let (snapshot, _, _, handles) = paralelas(2);
    let antes = snapshot.atomos();
    let id = identidad(&snapshot, handles[0]);
    let alta_sin_retiro = Delta::nuevo(
        [
            edge(&snapshot, 0),
            haslab_add(id.clone(), "PRIVATE_NEW_LABEL"),
        ],
        [],
    )
    .unwrap();
    let error = construir_candidato(&snapshot, &alta_sin_retiro).unwrap_err();
    assert!(matches!(
        error,
        ErrorCandidato::AddConflict {
            clase: ClaseOperacion::HasLab
        }
    ));
    assert!(!format!("{error} {error:?}").contains("PRIVATE_NEW_LABEL"));

    let retiro_sin_alta = Delta::nuevo(
        [],
        [Atomo::HasLab {
            sujeto: Sujeto::Arista(id),
            etiqueta: Etiqueta::nuevo("label-0"),
        }],
    )
    .unwrap();
    assert!(matches!(
        construir_candidato(&snapshot, &retiro_sin_alta),
        Err(ErrorCandidato::AddConflict {
            clase: ClaseOperacion::HasLab
        })
    ));
    assert_eq!(snapshot.atomos(), antes);
    snapshot.verificar_representacion().unwrap();
}

#[test]
fn altas_en_dos_grupos_independientes_preservan_ordinals_y_labels() {
    let mut snapshot = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let origen = snapshot
        .insertar_nodo(p("n/origen"), Sorte::nuevo("a"), Etiqueta::nuevo("origen"))
        .unwrap();
    snapshot
        .insertar_nodo(
            p("n/destino-a"),
            Sorte::nuevo("b"),
            Etiqueta::nuevo("destino-a"),
        )
        .unwrap();
    snapshot
        .insertar_nodo(
            p("n/destino-b"),
            Sorte::nuevo("b"),
            Etiqueta::nuevo("destino-b"),
        )
        .unwrap();
    let mut add = Vec::new();
    for (destino, ordinal, etiqueta) in [
        ("n/destino-a", 0, "a-0"),
        ("n/destino-a", 1, "a-1"),
        ("n/destino-b", 0, "b-0"),
    ] {
        let (id, edge) = edge_add("n/origen", destino, "rel", ordinal);
        add.push(edge);
        add.push(haslab_add(id, etiqueta));
    }
    let delta = Delta::nuevo(add.clone(), []).unwrap();
    let delta_reverso = Delta::nuevo(add.into_iter().rev(), []).unwrap();
    let salida = construir_candidato(&snapshot, &delta).unwrap();
    let salida_reversa = construir_candidato(&snapshot, &delta_reverso).unwrap();
    let handles = salida.estructura().salientes(origen);

    assert_eq!(handles.len(), 3);
    assert_eq!(
        salida.estructura().atomos(),
        salida_reversa.estructura().atomos()
    );
    for (handle, destino, ordinal, etiqueta) in [
        (handles[0], "n/destino-a", 0, "a-0"),
        (handles[1], "n/destino-a", 1, "a-1"),
        (handles[2], "n/destino-b", 0, "b-0"),
    ] {
        let arista = salida.estructura().arista(handle).unwrap();
        assert_eq!(
            arista.destino,
            salida.estructura().por_path(&p(destino)).unwrap()
        );
        assert_eq!(arista.ordinal, ordinal);
        assert_eq!(arista.etiqueta.como_str(), etiqueta);
    }
    snapshot.verificar_representacion().unwrap();
    salida.estructura().verificar_representacion().unwrap();
    salida_reversa
        .estructura()
        .verificar_representacion()
        .unwrap();
}
