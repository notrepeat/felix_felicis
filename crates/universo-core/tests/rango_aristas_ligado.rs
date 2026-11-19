//! Restricción de rango por aristas con un extremo ya ligado por un
//! cuantificador exterior: la regla de los repositorios de criterio
//! («todo criterio tiene al menos un proyecto») debe costar del orden de las
//! aristas del nodo, no de todas las aristas. Con 2 000 criterios el recorrido
//! de todas las aristas por cada uno agotaba el presupuesto por defecto.
use universo_core::sexpr::parsear;
use universo_core::{BundlePath, Estructura, Etiqueta, Formula, Sentencia, Sorte, evaluar};

const N: usize = 2000;

/// `N` nodos `a`, cada uno con una arista `sub` hacia el único nodo `b`,
/// salvo los indicados en `sin_arista`.
fn estructura(sin_arista: &[usize]) -> Estructura {
    let mut e = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let nodo = |e: &mut Estructura, path: &str, sorte: &str| {
        e.insertar_nodo(
            BundlePath::nuevo(path).unwrap(),
            Sorte::nuevo(sorte),
            Etiqueta::nuevo(path),
        )
        .unwrap()
    };
    let centro = nodo(&mut e, "n/centro", "b");
    for i in 0..N {
        let a = nodo(&mut e, &format!("n/a{i:05}"), "a");
        if !sin_arista.contains(&i) {
            e.insertar_arista(a, centro, Sorte::nuevo("sub"), Etiqueta::nuevo(""))
                .unwrap();
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

const ORIGEN_LIGADO: &str = "(forall ((x N)) (=> (Type x a) (al-menos 1 ((e E) (y N)) :guarda (Edge e x y sub) (Type y b))))";
const DESTINO_LIGADO: &str = "(forall ((y N)) (=> (Type y b) (al-menos 2000 ((e E) (x N)) :guarda (Edge e x y sub) (Type x a))))";

#[test]
fn con_el_origen_ligado_basta_el_presupuesto_por_defecto() {
    assert_eq!(evaluar(&formula(ORIGEN_LIGADO), &estructura(&[])), Ok(true));
    assert_eq!(
        evaluar(&formula(ORIGEN_LIGADO), &estructura(&[N - 1])),
        Ok(false)
    );
}

#[test]
fn con_el_destino_ligado_cuenta_sus_aristas_entrantes() {
    assert_eq!(
        evaluar(&formula(DESTINO_LIGADO), &estructura(&[])),
        Ok(true)
    );
    assert_eq!(
        evaluar(&formula(DESTINO_LIGADO), &estructura(&[7])),
        Ok(false)
    );
}

/// Un extremo constante que no existe sigue siendo un error aunque el otro
/// extremo esté ligado y no tenga aristas: el atajo no lo oculta.
#[test]
fn un_extremo_que_no_se_resuelve_sigue_siendo_error() {
    let e = estructura(&[]);
    let texto = "(forall ((y N)) (=> (Type y a) (exists ((e E)) :guarda (Edge e @n/falta y sub) (Type y a))))";
    assert!(evaluar(&formula(texto), &e).is_err(), "{texto}");
}
