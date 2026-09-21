use proptest::prelude::*;
use universo_core::{BundlePath, Estructura, Etiqueta, NodoId, Sorte};

#[derive(Debug, Clone)]
enum Op {
    Insertar(u8),
    Renombrar(u8, u8),
    Quitar(u8),
    Arista(u8, u8),
    QuitarArista(u8),
    Padre(u8, u8),
    QuitarPadre(u8),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        any::<u8>().prop_map(Op::Insertar),
        (any::<u8>(), any::<u8>()).prop_map(|(a, b)| Op::Renombrar(a, b)),
        any::<u8>().prop_map(Op::Quitar),
        (any::<u8>(), any::<u8>()).prop_map(|(a, b)| Op::Arista(a, b)),
        any::<u8>().prop_map(Op::QuitarArista),
        (any::<u8>(), any::<u8>()).prop_map(|(a, b)| Op::Padre(a, b)),
        any::<u8>().prop_map(Op::QuitarPadre),
    ]
}

fn path(n: u8) -> BundlePath {
    // 16 paths distintos, dos de ellos solo distintos por mayúsculas para ejercitar la colisión de case
    let nombres = [
        "p0", "p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8", "p9", "p10", "p11", "p12", "P0",
        "P1", "sub/p2",
    ];
    BundlePath::nuevo(&format!("n/{}", nombres[n as usize % 16])).unwrap()
}

fn elegir(vivos: &[NodoId], k: u8) -> Option<NodoId> {
    if vivos.is_empty() {
        None
    } else {
        Some(vivos[k as usize % vivos.len()])
    }
}

proptest! {
    #[test]
    fn representacion_coherente_bajo_cualquier_secuencia(ops in proptest::collection::vec(op(), 0..80)) {
        let mut u = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
        let mut vivos: Vec<NodoId> = Vec::new();
        for o in ops {
            match o {
                Op::Insertar(n) => if let Ok(id) = u.insertar_nodo(path(n), Sorte::nuevo("a"), Etiqueta::nuevo("x")) { vivos.push(id) },
                Op::Renombrar(k, n) => if let Some(id) = elegir(&vivos, k) { let _ = u.renombrar(id, path(n)); },
                Op::Quitar(k) => if let Some(id) = elegir(&vivos, k) { u.quitar_nodo(id).unwrap(); vivos.retain(|v| *v != id); },
                Op::Arista(a, b) => if let (Some(x), Some(y)) = (elegir(&vivos, a), elegir(&vivos, b)) { u.insertar_arista(x, y, Sorte::nuevo("rel"), Etiqueta::nuevo("")).unwrap(); },
                Op::QuitarArista(k) => { let todas: Vec<_> = u.aristas().map(|(id, _)| id).collect(); if !todas.is_empty() { u.quitar_arista(todas[k as usize % todas.len()]).unwrap(); } },
                Op::Padre(a, b) => if let (Some(x), Some(y)) = (elegir(&vivos, a), elegir(&vivos, b)) { let _ = u.asignar_padre(x, y); },
                Op::QuitarPadre(k) => if let Some(id) = elegir(&vivos, k) { u.quitar_padre(id).unwrap(); },
            }
            let informe = u.verificar_representacion();
            prop_assert!(informe.is_ok(), "{:?}", informe);
        }
        // Biyección final explícita: index[p] == id ⇔ arena[id].path == p
        for (id, n) in u.nodos() { prop_assert_eq!(u.por_path(&n.path), Some(id)); }
        prop_assert_eq!(u.nodos().count(), vivos.len());
    }
}
