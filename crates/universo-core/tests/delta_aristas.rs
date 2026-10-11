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
    (estructura, origen, handles)
}

fn edge(snapshot: &Estructura, ordinal: u32) -> Atomo {
    snapshot
        .atomos()
        .into_iter()
        .find(|atomo| matches!(atomo, Atomo::Edge { id, .. } if id.ordinal == ordinal))
        .unwrap()
}

#[test]
fn tres_paralelas_remove_cero_compacta_en_orden_y_conserva_labels_por_handle() {
    let (snapshot, origen, handles) = paralelas(3);
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
fn lote_reordena_retiros_de_forma_determinista_y_reasocia_label_al_handle() {
    let (snapshot, origen, handles) = paralelas(11);
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
    let (snapshot, _, _) = paralelas(3);
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
    let (snapshot, _, handles) = paralelas(1);
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
    assert!(
        construir_candidato(&snapshot, &delta)
            .unwrap()
            .estructura()
            .arista(handles[0])
            .is_none()
    );

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
        Err(ErrorCandidato::UnsupportedOperation {
            clase: ClaseOperacion::HasLab,
            ..
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
