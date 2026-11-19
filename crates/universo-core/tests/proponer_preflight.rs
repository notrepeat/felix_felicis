use universo_core::{
    Atomo, BundlePath, ClaseOperacion, Delta, ErrorPropuesta, Estructura, Etiqueta, IdArista,
    ResultadoPropuesta, Sorte, Sujeto, proponer,
};

fn p(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn base() -> Estructura {
    let mut estructura = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let raiz = estructura
        .insertar_nodo(
            p("n/raiz"),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("RAIZ_PRIVADA"),
        )
        .unwrap();
    let hijo = estructura
        .insertar_nodo(
            p("n/hijo"),
            Sorte::nuevo("b"),
            Etiqueta::nuevo("HIJO_PRIVADO"),
        )
        .unwrap();
    estructura
        .insertar_nodo(
            p("n/otro"),
            Sorte::nuevo("b"),
            Etiqueta::nuevo("OTRO_PRIVADO"),
        )
        .unwrap();
    estructura.asignar_padre(hijo, raiz).unwrap();
    estructura
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

fn edge(origen: &str, destino: &str, ordinal: u32) -> Atomo {
    let origen = p(origen);
    let destino = p(destino);
    let sorte = Sorte::nuevo("rel");
    let id = IdArista {
        origen: origen.clone(),
        sorte: sorte.clone(),
        destino: destino.clone(),
        ordinal,
    };
    Atomo::Edge {
        id,
        origen,
        destino,
        sorte,
    }
}

fn assert_tecnico(
    snapshot: &Estructura,
    delta: Delta,
    codigo: &str,
    canarios: &[&str],
) -> ErrorPropuesta {
    let antes = snapshot.atomos();
    let error = proponer(snapshot, &delta, &[]).unwrap_err();
    assert_eq!(error.codigo(), codigo);
    assert_eq!(error.etapa(), universo_core::EtapaErrorPropuesta::Candidato);
    assert!(!format!("{error} {error:?}").contains("RechazadaConflictos"));
    for canario in canarios {
        assert!(!format!("{error} {error:?}").contains(canario));
    }
    assert_eq!(snapshot.atomos(), antes);
    error
}

#[test]
fn invalidez_tecnica_tiene_precedencia_sobre_conflicto_funcional() {
    let snapshot = base();
    let caso_label = Delta::nuevo(
        [
            haslab("n/raiz", "NUEVA_LABEL_PRIVADA"),
            haslab("n/AUSENTE_PRIVADO", "LABEL_AUSENTE"),
        ],
        [],
    )
    .unwrap();
    assert_tecnico(
        &snapshot,
        caso_label,
        "subject_not_found",
        &["AUSENTE_PRIVADO", "NUEVA_LABEL_PRIVADA"],
    );

    let caso_ciclo =
        Delta::nuevo([parent("n/raiz", "n/hijo"), parent("n/hijo", "n/otro")], []).unwrap();
    assert_tecnico(
        &snapshot,
        caso_ciclo,
        "parent_cycle",
        &["n/raiz", "n/hijo", "n/otro"],
    );

    let caso_padre_ausente = Delta::nuevo(
        [
            parent("n/hijo", "n/otro"),
            parent("n/otro", "n/PADRE_AUSENTE_PRIVADO"),
        ],
        [],
    )
    .unwrap();
    assert_tecnico(
        &snapshot,
        caso_padre_ausente,
        "parent_reference_not_found",
        &["PADRE_AUSENTE_PRIVADO", "n/otro"],
    );

    assert!(matches!(
        proponer(&snapshot, &Delta::nuevo([], []).unwrap(), &[]).unwrap(),
        ResultadoPropuesta::Admitida(_)
    ));
}

#[test]
fn remove_not_found_prioriza_cualquier_conflicto_parent_del_mismo_plan() {
    let snapshot = base();
    let delta = Delta::nuevo(
        [parent("n/hijo", "n/otro"), parent("n/hijo", "n/raiz")],
        [parent("n/otro", "n/PADRE_AUSENTE_PRIVADO")],
    )
    .unwrap();
    let error = proponer(&snapshot, &delta, &[]).unwrap_err();
    assert_eq!(error.codigo(), "remove_not_found");
    assert_eq!(error.clase(), Some(ClaseOperacion::Parent));
    assert!(!format!("{error} {error:?}").contains("PADRE_AUSENTE_PRIVADO"));
}

#[test]
fn error_propuesta_no_contiene_causa_sensible_en_debug_ni_display() {
    let snapshot = base();
    let error: ErrorPropuesta = proponer(
        &snapshot,
        &Delta::nuevo([haslab("n/AUSENTE_PRIVADO", "ETIQUETA_PRIVADA")], []).unwrap(),
        &[],
    )
    .unwrap_err();
    assert_eq!(error.codigo(), "subject_not_found");
    let salida = format!("{error} {error:?}");
    assert!(!salida.contains("AUSENTE_PRIVADO"));
    assert!(!salida.contains("ETIQUETA_PRIVADA"));
}

#[test]
fn etiquetas_incompletas_son_tecnicas_aunque_coexistan_conflictos() {
    let snapshot = base();
    let falta_label = Delta::nuevo(
        [parent("n/hijo", "n/otro"), edge("n/raiz", "n/otro", 0)],
        [],
    )
    .unwrap();
    let error = assert_tecnico(
        &snapshot,
        falta_label,
        "label_missing",
        &["n/raiz", "n/otro"],
    );
    assert_eq!(error.clase(), Some(ClaseOperacion::HasLab));

    let id_huerfana = IdArista {
        origen: p("n/raiz"),
        sorte: Sorte::nuevo("rel"),
        destino: p("n/otro"),
        ordinal: 7,
    };
    let huerfana = Delta::nuevo(
        [
            parent("n/hijo", "n/otro"),
            Atomo::HasLab {
                sujeto: Sujeto::Arista(id_huerfana),
                etiqueta: Etiqueta::nuevo("LABEL_HUERFANA_PRIVADA"),
            },
        ],
        [],
    )
    .unwrap();
    let error = assert_tecnico(
        &snapshot,
        huerfana,
        "label_orphan",
        &["LABEL_HUERFANA_PRIVADA", "n/otro"],
    );
    assert_eq!(error.clase(), Some(ClaseOperacion::HasLab));

    let mut snapshot_con_arista = base();
    let origen = snapshot_con_arista.por_path(&p("n/raiz")).unwrap();
    let destino = snapshot_con_arista.por_path(&p("n/otro")).unwrap();
    snapshot_con_arista
        .insertar_arista(
            origen,
            destino,
            Sorte::nuevo("rel"),
            Etiqueta::nuevo("EDGE_LABEL_PRIVADA"),
        )
        .unwrap();
    let retiro_edge = Delta::nuevo(
        [parent("n/hijo", "n/otro")],
        [Atomo::HasLab {
            sujeto: Sujeto::Arista(IdArista {
                origen: p("n/raiz"),
                sorte: Sorte::nuevo("rel"),
                destino: p("n/otro"),
                ordinal: 0,
            }),
            etiqueta: Etiqueta::nuevo("EDGE_LABEL_PRIVADA"),
        }],
    )
    .unwrap();
    let error = assert_tecnico(
        &snapshot_con_arista,
        retiro_edge,
        "label_replacement_required",
        &["EDGE_LABEL_PRIVADA", "n/otro"],
    );
    assert_eq!(error.clase(), Some(ClaseOperacion::HasLab));

    let retiro_nodo = Delta::nuevo(
        [parent("n/hijo", "n/otro")],
        [haslab("n/hijo", "HIJO_PRIVADO")],
    )
    .unwrap();
    let error = assert_tecnico(
        &snapshot,
        retiro_nodo,
        "label_replacement_required",
        &["HIJO_PRIVADO", "n/otro"],
    );
    assert_eq!(error.clase(), Some(ClaseOperacion::HasLab));
}
