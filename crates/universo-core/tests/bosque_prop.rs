//! Fuzzing del invariante de bosque de ι (plan 10): la estructura acepta
//! exactamente lo que la simulación de referencia acepta.
use proptest::prelude::*;
use universo_core::{BundlePath, ErrorEstructura, Estructura, Etiqueta, NodoId, Sorte};
use universo_testkit::generadores::{es_bosque, mapa_iota, simular_iota};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn acepta_todo_bosque_y_rechaza_todo_ciclo((n, asignaciones) in mapa_iota(1..24usize)) {
        let mut u = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
        let ids: Vec<NodoId> = (0..n)
            .map(|i| u.insertar_nodo(BundlePath::nuevo(&format!("n/v{i}")).unwrap(), Sorte::nuevo("a"), Etiqueta::nuevo("")).unwrap())
            .collect();
        let mut rechazado = false;
        for (hijo, padre) in &asignaciones {
            match u.asignar_padre(ids[*hijo], ids[*padre]) {
                Ok(()) => {}
                Err(ErrorEstructura::CicloEnIota) | Err(ErrorEstructura::PadreDeSiMismo) => rechazado = true,
                Err(e) => prop_assert!(false, "error inesperado {e:?}"),
            }
        }
        let (padre_esperado, rechazo_esperado) = simular_iota(n, &asignaciones);
        prop_assert_eq!(rechazado, rechazo_esperado, "rechazo ⇔ la simulación rechaza");
        let padre_real: Vec<Option<usize>> = ids.iter().map(|id| u.padre(*id).map(|p| ids.iter().position(|x| *x == p).unwrap())).collect();
        prop_assert_eq!(&padre_real, &padre_esperado);
        prop_assert!(es_bosque(&padre_real), "ι resultante siempre es bosque");
        prop_assert!(u.verificar_representacion().is_ok());
    }
}

/// Regresión determinista: una cadena larga (0 es raíz, i → i-1) más una
/// asignación que cerraría el ciclo (0 → n-1) debe rechazarse exactamente
/// una vez y dejar la profundidad de la cadena intacta.
#[test]
fn cadena_larga_mas_cierre_de_ciclo_se_rechaza_una_vez() {
    let n = 23usize;
    let asignaciones: Vec<(usize, usize)> = (1..n)
        .map(|i| (i, i - 1))
        .chain(std::iter::once((0, n - 1)))
        .collect();
    let mut u = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let ids: Vec<NodoId> = (0..n)
        .map(|i| {
            u.insertar_nodo(
                BundlePath::nuevo(&format!("n/v{i}")).unwrap(),
                Sorte::nuevo("a"),
                Etiqueta::nuevo(""),
            )
            .unwrap()
        })
        .collect();
    let mut rechazos = 0;
    for (hijo, padre) in &asignaciones {
        match u.asignar_padre(ids[*hijo], ids[*padre]) {
            Ok(()) => {}
            Err(ErrorEstructura::CicloEnIota) | Err(ErrorEstructura::PadreDeSiMismo) => {
                rechazos += 1;
            }
            Err(e) => panic!("error inesperado {e:?}"),
        }
    }
    assert_eq!(rechazos, 1);
    let padre_real: Vec<Option<usize>> = ids
        .iter()
        .map(|id| {
            u.padre(*id)
                .map(|p| ids.iter().position(|x| *x == p).unwrap())
        })
        .collect();
    assert_eq!(padre_real[0], None);
    assert_eq!(padre_real[22], Some(21));
}
