use universo_core::{
    Anexo, Atomo, BundlePath, ClaseOperacion, Delta, Entrada, ErrorCandidato, Estructura, Etiqueta,
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
                passthrough: vec![Entrada {
                    clave: "x-private-frontmatter".into(),
                    crudo: "x-private-frontmatter: literal\n".into(),
                }],
            },
        )
        .unwrap();
    estructura.asignar_padre(destino, origen).unwrap();
    estructura
        .asignar_atributo(origen, "Nivel", "alto")
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
    assert_eq!(
        candidato.padre(candidato.por_path(&p("n/destino")).unwrap()),
        Some(origen)
    );
    assert_eq!(candidato.atributos(origen).get("Nivel").unwrap(), "alto");
    let anexo = candidato.anexo(origen).unwrap();
    assert_eq!(anexo.cuerpo, "cuerpo");
    assert_eq!(anexo.passthrough[0].clave, "x-private-frontmatter");
    assert_eq!(
        anexo.passthrough[0].crudo,
        "x-private-frontmatter: literal\n"
    );
    assert_eq!(entrada.anexo(origen), Some(anexo));
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
        [Atomo::Type {
            sujeto: p("n/PRIVATE_PATH"),
            sorte: Sorte::nuevo("PRIVATE_TYPE"),
        }],
        [edge(&entrada, arista)],
    )
    .unwrap();

    let error = construir_candidato(&entrada, &delta).unwrap_err();
    assert!(matches!(
        error,
        ErrorCandidato::UnsupportedOperation {
            lado: LadoDelta::Add,
            clase: ClaseOperacion::Type
        }
    ));
    let texto = format!("{error} {error:?}");
    assert!(texto.contains("unsupported_operation"));
    for canario in ["PRIVATE_PATH", "PRIVATE_TYPE"] {
        assert!(!texto.contains(canario));
    }
    assert_eq!(entrada.atomos(), antes);
    entrada.verificar_representacion().unwrap();
}

#[test]
fn type_sigue_fail_closed_en_ambos_lados_y_sin_mutar() {
    let (entrada, _, _, _) = base();
    let antes = entrada.atomos();
    let casos = [(
        Atomo::Type {
            sujeto: p("n/PRIVATE_PATH"),
            sorte: Sorte::nuevo("PRIVATE_TYPE"),
        },
        ClaseOperacion::Type,
    )];

    for (atomo, clase) in casos {
        for (delta, lado) in [
            (Delta::nuevo([atomo.clone()], []).unwrap(), LadoDelta::Add),
            (Delta::nuevo([], [atomo]).unwrap(), LadoDelta::Remove),
        ] {
            let error = construir_candidato(&entrada, &delta).unwrap_err();
            assert!(matches!(
                error,
                ErrorCandidato::UnsupportedOperation {
                    lado: actual_lado,
                    clase: actual_clase
                } if actual_lado == lado && actual_clase == clase
            ));
            let texto = format!("{error} {error:?}");
            for canario in ["PRIVATE_PATH", "PRIVATE_TYPE"] {
                assert!(!texto.contains(canario));
            }
            assert_eq!(entrada.atomos(), antes);
        }
    }
    entrada.verificar_representacion().unwrap();
}
