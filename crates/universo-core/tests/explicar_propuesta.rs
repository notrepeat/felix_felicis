//! Explicación redactada del rechazo de una propuesta, sobre el candidato.
use universo_core::sexpr::parsear_programa;
use universo_core::veredicto::{ModoExplicacion, ResultadoExplicaciones};
use universo_core::{Atomo, BundlePath, Delta, Estructura, Etiqueta, Sorte, explicar_propuesta};

fn p(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn base() -> Estructura {
    let mut estructura = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    estructura
        .insertar_nodo(p("n/A"), Sorte::nuevo("a"), Etiqueta::nuevo("A"))
        .unwrap();
    estructura
}

fn tipo(path: &str, sorte: &str) -> Atomo {
    Atomo::Type {
        sujeto: p(path),
        sorte: Sorte::nuevo(sorte),
    }
}

#[test]
fn explica_la_regla_que_el_candidato_rompe_no_el_estado_actual() {
    let fuentes = parsear_programa("(Type @n/A a)\n(regla :id \"r2\" (Type @n/A a))\n").unwrap();
    // El estado actual cumple ambas reglas; el candidato (Type a -> b) las rompe.
    let delta = Delta::nuevo([tipo("n/A", "b")], [tipo("n/A", "a")]).unwrap();
    let medido = explicar_propuesta(
        &base(),
        &delta,
        &fuentes,
        ModoExplicacion::Enumerar {
            maximo_resultados: 100,
        },
        1_000_000,
    )
    .expect("el candidato se construye")
    .expect("hay presupuesto");
    let ResultadoExplicaciones::Enumeracion { objetos, .. } = medido.resultado else {
        panic!("se pidió enumerar");
    };
    let fuentes_rotas: Vec<_> = objetos.iter().map(|o| o.procedencia.source_index).collect();
    assert_eq!(fuentes_rotas, vec![0, 1]);
}

#[test]
fn candidato_que_cumple_no_tiene_objetos_y_uno_imposible_no_se_explica() {
    let fuentes = parsear_programa("(Type @n/A a)\n").unwrap();
    let vacio = Delta::default();
    let medido = explicar_propuesta(
        &base(),
        &vacio,
        &fuentes,
        ModoExplicacion::Decidir,
        1_000_000,
    )
    .unwrap()
    .unwrap();
    assert!(matches!(
        medido.resultado,
        ResultadoExplicaciones::Decision(None)
    ));
    // Retirar un átomo inexistente no construye candidato: no hay nada que explicar.
    let imposible = Delta::nuevo([], [tipo("n/A", "b")]).unwrap();
    assert!(
        explicar_propuesta(
            &base(),
            &imposible,
            &fuentes,
            ModoExplicacion::Decidir,
            1_000_000
        )
        .is_none()
    );
}
