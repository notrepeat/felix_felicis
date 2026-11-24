//! Testigo localizado: primera asignación que falsea una oración universal.
use universo_core::sexpr::parsear;
use universo_core::testigo::{PresupuestoTestigo, testigo_de_oracion};
use universo_core::{BundlePath, Estructura, Etiqueta, Formula, Sentencia, Sorte, ValorLigadura};

/// n/a, n/b de sorte a; n/c de sorte b; aristas sub n/a→n/c y n/b→n/c.
fn estructura() -> Estructura {
    let mut e = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let mut ids = Vec::new();
    for (path, sorte) in [("n/a", "a"), ("n/b", "a"), ("n/c", "b")] {
        ids.push(
            e.insertar_nodo(
                BundlePath::nuevo(path).unwrap(),
                Sorte::nuevo(sorte),
                Etiqueta::nuevo(path),
            )
            .unwrap(),
        );
    }
    e.insertar_arista(ids[0], ids[2], Sorte::nuevo("sub"), Etiqueta::nuevo(""))
        .unwrap();
    e.insertar_arista(ids[1], ids[2], Sorte::nuevo("sub"), Etiqueta::nuevo(""))
        .unwrap();
    e
}

fn formula(texto: &str) -> Formula {
    let Sentencia::Oracion(f) = parsear(texto).unwrap() else {
        panic!()
    };
    f
}

fn testigo(texto: &str) -> Option<Vec<(String, String)>> {
    testigo_de_oracion(
        &formula(texto),
        &estructura(),
        &mut PresupuestoTestigo::nuevo(1_000_000, 100_000),
    )
    .unwrap()
    .map(|ligaduras| {
        ligaduras
            .into_iter()
            .map(|l| {
                let valor = match l.valor {
                    ValorLigadura::Nodo(p) => p.to_string(),
                    ValorLigadura::Arista(id) => id.to_string(),
                };
                (l.variable.to_string(), valor)
            })
            .collect()
    })
}

fn par(v: &str, x: &str) -> (String, String) {
    (v.to_string(), x.to_string())
}

#[test]
fn devuelve_la_primera_asignacion_falsa_en_orden_canonico() {
    assert_eq!(
        testigo("(forall ((x N)) (Type x a))"),
        Some(vec![par("x", "n/c")])
    );
    assert_eq!(
        testigo("(forall ((x N)) (Type x b))"),
        Some(vec![par("x", "n/a")])
    );
    assert_eq!(testigo("(forall ((x N)) (or (Type x a) (Type x b)))"), None);
}

#[test]
fn respeta_la_guarda_y_liga_aristas() {
    // Sólo cuentan las asignaciones que cumplen la guarda.
    assert_eq!(
        testigo("(forall ((e E) (x N) (y N)) :guarda (Edge e x y sub) (Type x b))"),
        Some(vec![
            par("e", "n/a→n/c#sub#0"),
            par("x", "n/a"),
            par("y", "n/c")
        ])
    );
    assert_eq!(
        testigo("(forall ((e E) (x N) (y N)) :guarda (Edge e x y sub) (Type y b))"),
        None
    );
}

#[test]
fn sigue_dentro_de_universales_anidados() {
    assert_eq!(
        testigo(
            "(forall ((x N)) :guarda (Type x a) (forall ((y N)) (not (exists ((e E)) (Edge e x y sub)))))"
        ),
        Some(vec![par("x", "n/a"), par("y", "n/c")])
    );
}

#[test]
fn una_oracion_falsa_sin_universal_da_testigo_vacio() {
    assert_eq!(testigo("(exists ((x N)) (Type x Retirado))"), Some(vec![]));
    assert_eq!(testigo("(exists ((x N)) (Type x a))"), None);
}

#[test]
fn coincide_con_el_evaluador_y_respeta_el_limite_de_instancias() {
    for texto in [
        "(forall ((x N) (y N)) (=> (Type x a) (Type y a)))",
        "(forall ((x N)) (al-menos 2 ((y N)) (Type y a)))",
        "(forall ((e E) (x N) (y N)) :guarda (Edge e x y sub) (not (= x y)))",
    ] {
        let f = formula(texto);
        let valor = universo_core::evaluar(&f, &estructura()).unwrap();
        assert_eq!(testigo(texto).is_none(), valor, "{texto}");
    }
    let f = formula("(forall ((x N) (y N)) (Type x a))");
    assert!(
        testigo_de_oracion(
            &f,
            &estructura(),
            &mut PresupuestoTestigo::nuevo(1_000_000, 2)
        )
        .is_err()
    );
    // El presupuesto de unidades es acumulado: alcanza para una búsqueda, no para muchas.
    let mut compartido = PresupuestoTestigo::nuevo(200, 1_000_000);
    let resultados: Vec<_> = (0..50)
        .map(|_| testigo_de_oracion(&f, &estructura(), &mut compartido))
        .collect();
    assert!(resultados[0].is_ok());
    assert!(resultados.iter().any(|r| r.is_err()));
}

// --- Todas las asignaciones falsas (`testigos_de_oracion`) ---

fn testigos(texto: &str, limite: usize) -> (Vec<Vec<(String, String)>>, bool) {
    let r = universo_core::testigo::testigos_de_oracion(
        &formula(texto),
        &estructura(),
        &mut PresupuestoTestigo::nuevo(1_000_000, 100_000),
        limite,
    )
    .unwrap();
    let asignaciones = r
        .asignaciones
        .into_iter()
        .map(|ligaduras| {
            ligaduras
                .into_iter()
                .map(|l| {
                    let valor = match l.valor {
                        ValorLigadura::Nodo(p) => p.to_string(),
                        ValorLigadura::Arista(id) => id.to_string(),
                    };
                    (l.variable.to_string(), valor)
                })
                .collect()
        })
        .collect();
    (asignaciones, r.truncado)
}

#[test]
fn todas_las_asignaciones_falsas_en_orden_canonico() {
    assert_eq!(
        testigos("(forall ((x N)) (Type x b))", 10),
        (vec![vec![par("x", "n/a")], vec![par("x", "n/b")]], false)
    );
    assert_eq!(
        testigos("(forall ((x N)) (or (Type x a) (Type x b)))", 10),
        (vec![], false)
    );
    // Una oración falsa sin universal: una asignación vacía.
    assert_eq!(
        testigos("(exists ((x N)) (Type x Retirado))", 10),
        (vec![vec![]], false)
    );
}

#[test]
fn la_guarda_acota_sin_cambiar_el_resultado() {
    // Guarda Edge: sólo las aristas `sub`, con sus extremos.
    assert_eq!(
        testigos(
            "(forall ((e E) (x N) (y N)) :guarda (Edge e x y sub) (Type x b))",
            10
        ),
        (
            vec![
                vec![par("e", "n/a→n/c#sub#0"), par("x", "n/a"), par("y", "n/c")],
                vec![par("e", "n/b→n/c#sub#0"), par("x", "n/b"), par("y", "n/c")],
            ],
            false
        )
    );
    // Guarda Type y guarda `and` cuyo primer átomo acota.
    assert_eq!(
        testigos("(forall ((x N)) :guarda (Type x a) (Type x b))", 10).0,
        vec![vec![par("x", "n/a")], vec![par("x", "n/b")]]
    );
    assert_eq!(
        testigos(
            "(forall ((x N)) :guarda (and (Type x a) (Type x a)) (Type x b))",
            10
        )
        .0
        .len(),
        2
    );
    // Extremo constante en la guarda.
    assert_eq!(
        testigos(
            "(forall ((e E) (y N)) :guarda (Edge e @n/b y sub) (Type y a))",
            10
        )
        .0,
        vec![vec![par("e", "n/b→n/c#sub#0"), par("y", "n/c")]]
    );
}

#[test]
fn la_primera_coincide_con_el_testigo_de_una_sola_falla() {
    for texto in [
        "(forall ((x N)) (Type x a))",
        "(forall ((x N)) (Type x b))",
        "(forall ((e E) (x N) (y N)) :guarda (Edge e x y sub) (Type x b))",
        "(forall ((x N)) :guarda (Type x a) (forall ((y N)) (not (exists ((e E)) (Edge e x y sub)))))",
        "(forall ((x N) (y N)) (=> (Type x a) (Type y a)))",
        "(forall ((e E) (x N) (y N)) :guarda (Edge e x y sub) (not (= x y)))",
    ] {
        let todas = testigos(texto, 100).0;
        assert_eq!(todas.first().cloned(), testigo(texto), "{texto}");
        // Verdadera si y sólo si no hay asignaciones falsas.
        let valor = universo_core::evaluar(&formula(texto), &estructura()).unwrap();
        assert_eq!(todas.is_empty(), valor, "{texto}");
    }
}

#[test]
fn respeta_el_limite_y_el_presupuesto() {
    assert_eq!(
        testigos("(forall ((x N)) (Type x b))", 1),
        (vec![vec![par("x", "n/a")]], true)
    );
    assert_eq!(testigos("(forall ((x N)) (Type x b))", 0), (vec![], true));
    // Sin guarda que acote, el producto se mide antes de construirlo.
    let f = formula("(forall ((x N) (y N) (z N)) (Type x a))");
    assert!(
        universo_core::testigo::testigos_de_oracion(
            &f,
            &estructura(),
            &mut PresupuestoTestigo::nuevo(1_000_000, 5),
            10
        )
        .is_err()
    );
}
