use proptest::prelude::*;

use crate::estructura::Estructura;
use crate::evaluador::evaluar;
use crate::tipos::{Etiqueta, Sorte};

use super::oracle::{AsignacionOraculo, oraculo};
use super::{camino, manifiesto_prueba, oracion};

fn mundo_generado(n: usize, semilla: u64) -> Estructura {
    let mut estructura = Estructura::nueva(manifiesto_prueba()).unwrap();
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let tipo = if (semilla.rotate_left(i as u32) & 1) == 0 {
            "a"
        } else {
            "b"
        };
        let etiqueta = if i == 0 || (semilla.rotate_right(i as u32) & 2) == 0 {
            "L"
        } else {
            ""
        };
        let id = estructura
            .insertar_nodo(
                camino(&format!("n/v{i}")),
                Sorte::nuevo(tipo),
                Etiqueta::nuevo(etiqueta),
            )
            .unwrap();
        ids.push(id);
    }
    for hijo in 1..n {
        if (semilla.rotate_left((hijo + 8) as u32) & 1) != 0 {
            let padre = (semilla.rotate_right((hijo + 3) as u32) as usize) % hijo;
            estructura.asignar_padre(ids[hijo], ids[padre]).unwrap();
        }
    }
    for i in 0..(semilla as usize % 5) {
        let origen = (semilla.rotate_left(i as u32) as usize) % n;
        let destino = (semilla.rotate_right((i + 2) as u32) as usize) % n;
        let sorte = if (semilla.rotate_right((i + 4) as u32) & 1) == 0 {
            "rel"
        } else {
            "sub"
        };
        let etiqueta = if (semilla.rotate_left((i + 5) as u32) & 1) == 0 {
            "L"
        } else {
            ""
        };
        estructura
            .insertar_arista(
                ids[origen],
                ids[destino],
                Sorte::nuevo(sorte),
                Etiqueta::nuevo(etiqueta),
            )
            .unwrap();
    }
    for (i, id) in ids.iter().enumerate() {
        if (semilla.rotate_left((i + 17) as u32) & 1) != 0 {
            let valor = if (semilla.rotate_right((i + 11) as u32) & 1) == 0 {
                "alto"
            } else {
                "bajo"
            };
            estructura.asignar_atributo(*id, "Nivel", valor).unwrap();
        }
    }
    estructura
}

#[test]
fn el_evaluador_coincide_con_el_oraculo_en_estructuras_generadas() {
    let corpus = [
        "(forall ((x N)) (or (Type x a) (Type x b)))",
        "(exists ((x N)) (Type x a))",
        "(forall ((x N) (y N)) (=> (Parent x y) (not (= x y))))",
        "(exists ((e E) (x N) (y N)) (Edge e x y rel))",
        "(forall ((e E)) (HasLab e \"L\"))",
        "(forall ((x N)) :guarda (Type x b) (Type x b))",
        "(exists ((x N)) :guarda (Type x b) (Type x a))",
        "(exists ((x N) (y N)) (tc Parent x y))",
        "(forall ((x N)) (=> (Type x a) (HasLab x \"L\")))",
        "(exists ((x N)) (Nivel x alto))",
        "(forall ((e E) (x N) (y N)) (=> (Edge e x y rel) (HasLab e \"L\")))",
    ];
    proptest!(|(n in 1usize..6, semilla in any::<u64>())| {
        let estructura = mundo_generado(n, semilla);
        for texto in corpus {
            let formula = oracion(texto);
            let esperado = oraculo(&formula, &estructura, &mut AsignacionOraculo::default())
                .expect("el corpus solo usa símbolos declarados");
            let real = evaluar(&formula, &estructura)
                .expect("el corpus solo usa símbolos declarados");
            prop_assert_eq!(
                real,
                esperado,
                "fórmula {:?}; semilla {}; n={}",
                texto,
                semilla,
                n
            );
        }
    });
}
