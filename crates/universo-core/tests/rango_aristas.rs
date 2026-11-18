//! Restricción de rango por aristas en el veredicto: una guarda `(Edge e x y k)`
//! recorre las aristas de `k` en vez del producto E × N × N.
use universo_core::sexpr::parsear;
use universo_core::{
    BundlePath, Estructura, Etiqueta, Formula, Sentencia, Sorte, evaluar, evaluar_con_presupuesto,
    evaluar_trazable,
};

/// `n` nodos de sorte a y tres aristas `rel`.
fn estructura(n: usize) -> Estructura {
    let mut e = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let ids: Vec<_> = (0..n)
        .map(|i| {
            e.insertar_nodo(
                BundlePath::nuevo(&format!("n/x{i:04}")).unwrap(),
                Sorte::nuevo("a"),
                Etiqueta::nuevo("x"),
            )
            .unwrap()
        })
        .collect();
    for (o, d) in [(0, 1), (2, 3), (4, 4)] {
        e.insertar_arista(ids[o], ids[d], Sorte::nuevo("sub"), Etiqueta::nuevo(""))
            .unwrap();
    }
    e
}

fn formula(texto: &str) -> Formula {
    let Sentencia::Oracion(f) = parsear(texto).unwrap() else {
        panic!()
    };
    f
}

#[test]
fn la_guarda_de_arista_no_recorre_el_producto_de_nodos() {
    let grande = estructura(300);
    let guardada = formula("(forall ((e E) (x N) (y N)) :guarda (Edge e x y sub) (Type x a))");
    // 300 × 300 × 3 = 270 000 asignaciones: con 20 000 unidades sólo cabe
    // recorrer las aristas.
    assert!(evaluar_con_presupuesto(&guardada, &grande, 20_000).unwrap());
    let sin_guarda = formula("(forall ((e E) (x N) (y N)) (=> (Edge e x y sub) (Type x a)))");
    assert!(evaluar_con_presupuesto(&sin_guarda, &grande, 20_000).is_err());
    // En una estructura pequeña, donde cabe el producto, las dos formas coinciden.
    let chica = estructura(8);
    assert_eq!(
        evaluar(&sin_guarda, &chica).unwrap(),
        evaluar(&guardada, &chica).unwrap()
    );
}

#[test]
fn el_veredicto_por_aristas_coincide_con_el_trazable_y_sus_cortes() {
    let e = estructura(6);
    for (texto, esperado) in [
        (
            "(forall ((e E) (x N) (y N)) :guarda (Edge e x y sub) (not (= x y)))",
            false,
        ),
        (
            "(exists ((e E) (x N)) :guarda (Edge e x x sub) (Type x a))",
            true,
        ),
        (
            "(al-menos 3 ((e E) (x N) (y N)) :guarda (Edge e x y sub) (Type y a))",
            true,
        ),
        (
            "(al-menos 4 ((e E) (x N) (y N)) :guarda (Edge e x y sub) (Type y a))",
            false,
        ),
        (
            "(forall ((e E) (y N)) :guarda (Edge e @n/x0000 y sub) (= y @n/x0001))",
            true,
        ),
        (
            "(forall ((x N)) (forall ((e E) (y N)) :guarda (Edge e x y sub) (= y x)))",
            false,
        ),
        (
            "(forall ((e E) (x N) (y N)) :guarda (and (Edge e x y sub) (not (= x y))) (Type y a))",
            true,
        ),
    ] {
        let f = formula(texto);
        assert_eq!(evaluar(&f, &e).unwrap(), esperado, "{texto}");
        assert_eq!(evaluar_trazable(&f, &e).unwrap().valor, esperado, "{texto}");
    }
}

/// Aristas `sub` a_i → a_{i+1} insertadas en orden inverso: el contraejemplo
/// (origen a00) es el primero en orden canónico y el último en inserción.
#[test]
fn lo_que_cabia_con_el_recorrido_completo_sigue_cabiendo() {
    let mut e = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let ids: Vec<_> = (0..12)
        .map(|i| {
            e.insertar_nodo(
                BundlePath::nuevo(&format!("n/a{i:02}")).unwrap(),
                Sorte::nuevo("a"),
                Etiqueta::nuevo("x"),
            )
            .unwrap()
        })
        .collect();
    for i in (0..11).rev() {
        e.insertar_arista(ids[i], ids[i + 1], Sorte::nuevo("sub"), Etiqueta::nuevo(""))
            .unwrap();
    }
    let cuerpo = "(and (not (= x @n/a00)) (forall ((z N)) (forall ((w N)) (= z z))))";
    let por_aristas = formula(&format!(
        "(forall ((e E) (x N) (y N)) :guarda (Edge e x y sub) {cuerpo})"
    ));
    let completo = formula(&format!(
        "(forall ((e E) (x N) (y N)) :guarda (and (Type x a) (Edge e x y sub)) {cuerpo})"
    ));
    let minimo = (1..100_000)
        .find(|&limite| evaluar_con_presupuesto(&completo, &e, limite).is_ok())
        .unwrap();
    assert_eq!(evaluar_con_presupuesto(&completo, &e, minimo), Ok(false));
    assert_eq!(evaluar_con_presupuesto(&por_aristas, &e, minimo), Ok(false));
}
