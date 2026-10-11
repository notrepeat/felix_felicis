use universo_core::{
    Atomo, BundlePath, ClaseOperacion, Delta, ErrorCandidato, Estructura, Etiqueta, Sorte,
    construir_candidato,
};

fn p(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn base() -> (Estructura, universo_core::NodoId, universo_core::NodoId) {
    let mut estructura = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let nodo = estructura
        .insertar_nodo(p("n/nodo"), Sorte::nuevo("a"), Etiqueta::nuevo("nodo"))
        .unwrap();
    let otro = estructura
        .insertar_nodo(p("n/otro"), Sorte::nuevo("b"), Etiqueta::nuevo("otro"))
        .unwrap();
    (estructura, nodo, otro)
}

fn ext(sujeto: &str, constante: &str) -> Atomo {
    Atomo::Ext {
        predicado: "Nivel".into(),
        sujeto: p(sujeto),
        constante: constante.into(),
    }
}

fn assert_redactado(error: &ErrorCandidato, canarios: &[&str]) {
    let texto = format!("{error} {error:?}");
    for canario in canarios {
        assert!(!texto.contains(canario), "se filtró {canario}: {texto}");
    }
}

#[test]
fn alta_ext_sin_vigente_e_identica_son_idempotentes() {
    let (snapshot, nodo, _) = base();
    let antes = snapshot.atomos();
    let delta = Delta::nuevo([ext("n/nodo", "alto")], []).unwrap();
    let candidato = construir_candidato(&snapshot, &delta).unwrap();

    assert_eq!(
        candidato.estructura().atributos(nodo).get("Nivel"),
        Some(&"alto".into())
    );
    assert_eq!(snapshot.atomos(), antes);

    let delta_identico = Delta::nuevo([ext("n/nodo", "alto")], []).unwrap();
    let no_op = construir_candidato(candidato.estructura(), &delta_identico).unwrap();
    assert_eq!(no_op.estructura().atomos(), candidato.estructura().atomos());
    assert_eq!(snapshot.atomos(), antes);
    no_op.estructura().verificar_representacion().unwrap();
}

#[test]
fn remove_ext_exacto_elimina_solo_el_atributo_del_sujeto() {
    let (mut snapshot, nodo, otro) = base();
    snapshot.asignar_atributo(nodo, "Nivel", "alto").unwrap();
    snapshot.asignar_atributo(otro, "Nivel", "bajo").unwrap();
    let antes = snapshot.atomos();
    let delta = Delta::nuevo([], [ext("n/nodo", "alto")]).unwrap();

    let candidato = construir_candidato(&snapshot, &delta).unwrap();

    assert!(!candidato.estructura().atributos(nodo).contains_key("Nivel"));
    assert_eq!(
        candidato
            .estructura()
            .atributos(otro)
            .get("Nivel")
            .map(String::as_str),
        Some("bajo")
    );
    assert_eq!(snapshot.atomos(), antes);
    candidato.estructura().verificar_representacion().unwrap();
}

#[test]
fn reemplazo_ext_exige_retiro_vigente_exacto_y_alta() {
    let (mut snapshot, nodo, _) = base();
    snapshot.asignar_atributo(nodo, "Nivel", "bajo").unwrap();
    let antes = snapshot.atomos();

    let sin_retiro = Delta::nuevo([ext("n/nodo", "alto")], []).unwrap();
    let error = construir_candidato(&snapshot, &sin_retiro).unwrap_err();
    assert!(matches!(
        error,
        ErrorCandidato::AddConflict {
            clase: ClaseOperacion::Ext
        }
    ));

    let reemplazo = Delta::nuevo([ext("n/nodo", "alto")], [ext("n/nodo", "bajo")]).unwrap();
    let candidato = construir_candidato(&snapshot, &reemplazo).unwrap();
    assert_eq!(
        candidato
            .estructura()
            .atributos(nodo)
            .get("Nivel")
            .map(String::as_str),
        Some("alto")
    );
    assert_eq!(snapshot.atomos(), antes);
}

#[test]
fn retiro_ausente_o_con_valor_incorrecto_es_remove_not_found_redactado() {
    let (mut snapshot, nodo, _) = base();
    snapshot.asignar_atributo(nodo, "Nivel", "alto").unwrap();
    let antes = snapshot.atomos();

    for retiro in [ext("n/nodo", "bajo"), ext("n/ausente", "alto")] {
        let delta = Delta::nuevo([], [retiro]).unwrap();
        let error = construir_candidato(&snapshot, &delta).unwrap_err();
        assert!(matches!(
            error,
            ErrorCandidato::RemoveNotFound {
                clase: ClaseOperacion::Ext
            }
        ));
        assert_redactado(&error, &["n/nodo", "n/ausente", "bajo", "alto"]);
        assert_eq!(snapshot.atomos(), antes);
    }
}

#[test]
fn predicado_sujeto_y_dominio_invalidos_rechazan_redactados() {
    let (snapshot, _, _) = base();
    let antes = snapshot.atomos();
    let invalidos = [
        Atomo::Ext {
            predicado: "PREDICADO_PRIVADO".into(),
            sujeto: p("n/nodo"),
            constante: "VALOR_PRIVADO".into(),
        },
        Atomo::Ext {
            predicado: "Nivel".into(),
            sujeto: p("n/SUJETO_PRIVADO"),
            constante: "alto".into(),
        },
        Atomo::Ext {
            predicado: "Nivel".into(),
            sujeto: p("n/nodo"),
            constante: "VALOR_PRIVADO".into(),
        },
    ];

    for atomo in invalidos {
        let delta = Delta::nuevo([atomo], []).unwrap();
        let error = construir_candidato(&snapshot, &delta).unwrap_err();
        assert!(matches!(
            error,
            ErrorCandidato::AddConflict {
                clase: ClaseOperacion::Ext
            }
        ));
        assert_redactado(
            &error,
            &["PREDICADO_PRIVADO", "SUJETO_PRIVADO", "VALOR_PRIVADO"],
        );
        assert_eq!(snapshot.atomos(), antes);
    }
}

#[test]
fn valores_ext_conflictivos_en_el_mismo_sujeto_rechazan_redactados() {
    let (snapshot, _, _) = base();
    let antes = snapshot.atomos();
    let delta = Delta::nuevo([ext("n/nodo", "alto"), ext("n/nodo", "bajo")], []).unwrap();

    let error = construir_candidato(&snapshot, &delta).unwrap_err();

    assert!(matches!(
        error,
        ErrorCandidato::AddConflict {
            clase: ClaseOperacion::Ext
        }
    ));
    assert_redactado(&error, &["n/nodo", "alto", "bajo"]);
    assert_eq!(snapshot.atomos(), antes);
}
