//! C4: cuantificador de cardinalidad `(al-menos n ((x N)) …)`.
use universo_core::evaluador::{ReglaEvidencia, TipoCuantificador, evaluar_trazable};
use universo_core::invariante::validar_sentencia_fuente;
use universo_core::sexpr::{parsear, parsear_programa};
use universo_core::{BundlePath, Estructura, Etiqueta, Formula, Sentencia, Sorte, evaluar};

/// Tres nodos `a` y uno `b`; dos de los `a` con `Nivel = alto`.
fn estructura() -> Estructura {
    let mut e = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    for (path, sorte, nivel) in [
        ("n/a1", "a", Some("alto")),
        ("n/a2", "a", Some("alto")),
        ("n/a3", "a", Some("bajo")),
        ("n/b1", "b", None),
    ] {
        let id = e
            .insertar_nodo(
                BundlePath::nuevo(path).unwrap(),
                Sorte::nuevo(sorte),
                Etiqueta::nuevo(path),
            )
            .unwrap();
        if let Some(v) = nivel {
            e.asignar_atributo(id, "Nivel", v).unwrap();
        }
    }
    e
}

fn formula(texto: &str) -> Formula {
    let Sentencia::Oracion(f) = parsear(texto).unwrap() else {
        panic!("{texto}")
    };
    f
}

fn vale(texto: &str) -> bool {
    evaluar(&formula(texto), &estructura()).unwrap()
}

#[test]
fn cuenta_testigos_distintos_con_y_sin_guarda() {
    assert!(vale("(al-menos 3 ((x N)) (Type x a))"));
    assert!(!vale("(al-menos 4 ((x N)) (Type x a))"));
    assert!(vale(
        "(al-menos 2 ((x N)) :guarda (Type x a) (Nivel x alto))"
    ));
    assert!(!vale(
        "(al-menos 3 ((x N)) :guarda (Type x a) (Nivel x alto))"
    ));
    assert!(vale("(al-menos 1 ((x N)) (Type x b))"));
    // Pares ordenados distintos: 3 × 3 asignaciones de dos `a`.
    assert!(vale(
        "(al-menos 9 ((x N) (y N)) (and (Type x a) (Type y a)))"
    ));
    assert!(!vale(
        "(al-menos 10 ((x N) (y N)) (and (Type x a) (Type y a)))"
    ));
}

#[test]
fn al_menos_uno_equivale_a_exists_y_se_anida() {
    for cuerpo in ["(Type x a)", "(Type x Retirado)", "(Nivel x alto)"] {
        assert_eq!(
            vale(&format!("(al-menos 1 ((x N)) {cuerpo})")),
            vale(&format!("(exists ((x N)) {cuerpo})")),
            "{cuerpo}"
        );
    }
    assert!(vale(
        "(forall ((x N)) :guarda (Type x b) (al-menos 2 ((y N)) (Nivel y alto)))"
    ));
    assert!(!vale("(not (al-menos 2 ((x N)) (Nivel x alto)))"));
}

#[test]
fn forma_canonica_se_reparsea() {
    let texto = "(al-menos 2 ((x N)) :guarda (Type x a) (Nivel x alto))";
    assert_eq!(formula(texto).to_string(), texto);
    assert_eq!(formula(&formula(texto).to_string()), formula(texto));
}

#[test]
fn rechaza_cardinalidades_mal_formadas() {
    for malo in [
        "(al-menos 0 ((x N)) (Type x a))",
        "(al-menos -1 ((x N)) (Type x a))",
        "(al-menos dos ((x N)) (Type x a))",
        "(al-menos 1.5 ((x N)) (Type x a))",
        "(al-menos 2 (Type x a))",
        "(al-menos ((x N)) (Type x a))",
    ] {
        assert!(parsear(malo).is_err(), "{malo}");
    }
}

#[test]
fn la_regla_valida_y_la_evidencia_se_corta_al_alcanzar_n() {
    let e = estructura();
    let fuentes = parsear_programa("(al-menos 2 ((x N)) (Nivel x alto))").unwrap();
    assert!(validar_sentencia_fuente(&fuentes[0], e.manifiesto(), &e).is_empty());

    let traza = evaluar_trazable(&formula("(al-menos 2 ((x N)) (Nivel x alto))"), &e).unwrap();
    assert!(traza.valor);
    let ReglaEvidencia::Cuantificador {
        tipo,
        instancias,
        busqueda_agotada,
    } = &traza.evidencia.regla
    else {
        panic!("se esperaba un cuantificador")
    };
    assert_eq!(*tipo, TipoCuantificador::AlMenos { minimo: 2 });
    // n/a1 y n/a2 bastan: no se recorre el resto.
    assert_eq!(instancias.len(), 2);
    assert!(!busqueda_agotada);

    let traza = evaluar_trazable(&formula("(al-menos 3 ((x N)) (Nivel x alto))"), &e).unwrap();
    assert!(!traza.valor);
    let ReglaEvidencia::Cuantificador {
        instancias,
        busqueda_agotada,
        ..
    } = &traza.evidencia.regla
    else {
        panic!()
    };
    assert_eq!(instancias.len(), 4);
    assert!(busqueda_agotada);
}

#[test]
fn el_ejemplo_del_readme_parsea() {
    let texto = "(regla :id \"revision-doble\" :mensaje \"Toda decisión necesita dos revisiones.\"\n  (forall ((d N)) :guarda (Type d decision)\n    (al-menos 2 ((r N)) (exists ((e E)) (Edge e r d reviews)))))";
    assert!(parsear_programa(texto).is_ok());
    let readme = include_str!("../../../README.md").replace("\r\n", "\n");
    assert!(readme.contains(texto));
}

#[test]
fn casos_limite_de_aridad_cardinalidad_y_dominio() {
    for (malo, fragmento) in [
        ("(al-menos)", "al-menos"),
        ("(al-menos 2 ((x N)))", "al-menos"),
        ("(al-menos 2 ((x N)) :guarda (Type x a))", "al-menos"),
        ("(al-menos 4294967296 ((x N)) (Type x a))", "4294967296"),
        ("(al-menos 01 ((x N)) (Type x a))", "01"),
        ("(al-menos +2 ((x N)) (Type x a))", "+2"),
        ("(al-menos \"2\" ((x N)) (Type x a))", "lista o una cadena"),
    ] {
        let error = parsear(malo).unwrap_err().to_string();
        assert!(error.contains(fragmento), "{malo}: {error}");
    }
    assert!(parsear("(al-menos 4294967295 ((x N)) (Type x a))").is_ok());

    // Sin ligaduras hay una sola asignación.
    assert!(vale("(al-menos 1 () (Type @n/a1 a))"));
    assert!(!vale("(al-menos 2 () (Type @n/a1 a))"));

    // Dominio vacío: ningún testigo.
    let vacia = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    assert!(!evaluar(&formula("(al-menos 1 ((x N)) (= x x))"), &vacia).unwrap());

    // Un AST construido a mano con mínimo 0 es trivialmente verdadero.
    let Formula::AlMenos {
        ligaduras,
        guarda,
        cuerpo,
        ..
    } = formula("(al-menos 1 ((x N)) (Type x Retirado))")
    else {
        panic!()
    };
    let cero = Formula::AlMenos {
        minimo: 0,
        ligaduras,
        guarda,
        cuerpo,
    };
    assert!(evaluar(&cero, &estructura()).unwrap());

    // Guarda falsa en algunas instancias del universal envolvente.
    assert!(!vale(
        "(forall ((x N)) :guarda (Type x a) (al-menos 2 ((y N)) :guarda (= x y) (Type y a)))"
    ));
}
