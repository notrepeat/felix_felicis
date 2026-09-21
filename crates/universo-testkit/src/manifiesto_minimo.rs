//! Manifiesto mínimo para tests (spec §5): dos sortes de nodo simples más
//! los reservados, tres claves de arista que ejercitan simetría, inversa
//! recíproca y la forma de lista de mappings, y un predicado con dominio
//! cerrado.

use std::collections::BTreeMap;

use universo_core::{DeclArista, DeclPredicado, Layout, Manifiesto, Sorte};

/// Manifiesto mínimo: $K_V$ = {a, b, Retirado, Patrón}; aristas `rel`
/// (simétrica), `sub`/`sup` (inversas recíprocas) y `ten` (simétrica, forma
/// de lista de mappings con destino `with` y etiqueta `scope`); predicado
/// `Nivel` con dominio {bajo, alto}; layout de un único directorio `n`.
pub fn manifiesto_minimo() -> Manifiesto {
    let mut sortes_nodo_disco = BTreeMap::new();
    sortes_nodo_disco.insert(Sorte::nuevo("Patrón"), "pattern".to_string());

    Manifiesto {
        version: "minimo/1".into(),
        sortes_nodo: vec![
            Sorte::nuevo("a"),
            Sorte::nuevo("b"),
            Sorte::nuevo("Retirado"),
            Sorte::nuevo("Patrón"),
        ],
        sortes_nodo_disco,
        retirado_en_disco: Some(("status".into(), "deprecated".into())),
        aristas: vec![
            DeclArista {
                clave: "rel".into(),
                simetrica: true,
                inversa: None,
                destino: None,
                etiqueta: None,
            },
            DeclArista {
                clave: "sub".into(),
                simetrica: false,
                inversa: Some("sup".into()),
                destino: None,
                etiqueta: None,
            },
            DeclArista {
                clave: "sup".into(),
                simetrica: false,
                inversa: Some("sub".into()),
                destino: None,
                etiqueta: None,
            },
            DeclArista {
                clave: "ten".into(),
                simetrica: true,
                inversa: None,
                destino: Some("with".into()),
                etiqueta: Some("scope".into()),
            },
        ],
        predicados: vec![DeclPredicado {
            nombre: "Nivel".into(),
            clave: "nivel".into(),
            dominio: ["bajo", "alto"].into_iter().map(String::from).collect(),
        }],
        layout: Layout {
            directorios: vec!["n".into()],
            sin_iota: vec!["n/libre".into()],
            ignorados: vec!["log.md".into()],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifiesto_minimo_es_valido() {
        manifiesto_minimo().validar().unwrap();
    }
}
