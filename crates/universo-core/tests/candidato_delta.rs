use universo_core::{
    Anexo, Atomo, BundlePath, ClaseOperacion, Delta, ErrorCandidato, Estructura, Etiqueta,
    IdArista, LadoDelta, Sorte, construir_candidato,
};

fn p(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn base() -> (
    Estructura,
    universo_core::NodoId,
    universo_core::NodoId,
    universo_core::AristaId,
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
    estructura
        .anexar(
            origen,
            Anexo {
                cuerpo: "cuerpo".into(),
                ..Anexo::default()
            },
        )
        .unwrap();
    let arista = estructura
        .insertar_arista(
            origen,
            destino,
            Sorte::nuevo("rel"),
            Etiqueta::nuevo("label"),
        )
        .unwrap();
    (estructura, origen, destino, arista)
}

fn edge(snapshot: &Estructura, handle: universo_core::AristaId) -> Atomo {
    let arista = snapshot.arista(handle).unwrap();
    let origen = snapshot.nodo(arista.origen).unwrap().path.clone();
    let destino = snapshot.nodo(arista.destino).unwrap().path.clone();
    let id = IdArista {
        origen: origen.clone(),
        sorte: arista.tipo.clone(),
        destino: destino.clone(),
        ordinal: arista.ordinal,
    };
    Atomo::Edge {
        id,
        origen,
        destino,
        sorte: arista.tipo.clone(),
    }
}

#[test]
fn candidato_y_entrada_son_aislados_y_preservan_anexos() {
    let (entrada, origen, _, arista) = base();
    let delta = Delta::nuevo([], [edge(&entrada, arista)]).unwrap();
    let antes = entrada.atomos();
    let candidato_aislado = construir_candidato(&entrada, &delta).unwrap();
    let debug = format!("{candidato_aislado:?}");
    assert!(debug.contains("edge_count: 0"));
    assert!(!debug.contains("n/origen"));
    let mut candidato = candidato_aislado.into_estructura();

    assert_eq!(entrada.atomos(), antes);
    assert!(entrada.arista(arista).is_some());
    assert!(candidato.arista(arista).is_none());
    assert_eq!(candidato.anexo(origen).unwrap().cuerpo, "cuerpo");
    candidato
        .insertar_nodo(
            p("n/candidato"),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("nuevo"),
        )
        .unwrap();
    assert_eq!(entrada.len_nodos(), 2);
    assert_eq!(entrada.verificar_representacion(), Ok(()));
    assert_eq!(candidato.verificar_representacion(), Ok(()));
}

#[test]
fn operacion_no_soportada_rechaza_sin_reflejar_datos_privados() {
    let (entrada, _, _, arista) = base();
    let antes = entrada.atomos();
    let delta = Delta::nuevo(
        [Atomo::Ext {
            predicado: "PRIVATE_PREDICATE".into(),
            sujeto: p("n/PRIVATE_PATH"),
            constante: "PRIVATE_VALUE".into(),
        }],
        [edge(&entrada, arista)],
    )
    .unwrap();

    let error = construir_candidato(&entrada, &delta).unwrap_err();
    assert!(matches!(
        error,
        ErrorCandidato::UnsupportedOperation {
            lado: LadoDelta::Add,
            clase: ClaseOperacion::Ext
        }
    ));
    let texto = format!("{error} {error:?}");
    assert!(texto.contains("unsupported_operation"));
    for canario in ["PRIVATE_PREDICATE", "PRIVATE_PATH", "PRIVATE_VALUE"] {
        assert!(!texto.contains(canario));
    }
    assert_eq!(entrada.atomos(), antes);
    entrada.verificar_representacion().unwrap();
}
