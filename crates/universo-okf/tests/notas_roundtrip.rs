//! Una nota creada en el núcleo es idéntica a la que el cargador OKF lee del
//! Markdown serializado: tipo, etiqueta, anexo y padre legado.
use universo_core::{BundlePath, Delta, Etiqueta, NotaNueva, Sorte, construir_candidato};
use universo_okf::{parsear_bundle, serializar_nodo};

fn nota(path: &str, tipo: &str, titulo: &str, cuerpo: &str) -> NotaNueva {
    NotaNueva {
        path: BundlePath::nuevo(path).unwrap(),
        tipo: Sorte::nuevo(tipo),
        etiqueta: Etiqueta::nuevo(titulo),
        cuerpo: cuerpo.to_string(),
    }
}

#[test]
fn notas_creadas_sobreviven_a_serializar_y_recargar() {
    let manifiesto = universo_testkit::manifiesto_minimo();
    let base = parsear_bundle(
        &[
            (
                "n/index.md".into(),
                "---\ntype: a\ntitle: Índice\n---\n".into(),
            ),
            (
                "n/existente.md".into(),
                "---\ntype: b\ntitle: Existente\n---\ncuerpo\n".into(),
            ),
        ],
        manifiesto.clone(),
    )
    .unwrap();
    let delta = Delta::default()
        .con_notas([
            nota(
                "n/hija",
                "a",
                "Hija con: dos puntos y \"comillas\"",
                "Texto.\n\n## Sección\n",
            ),
            nota("n/libre/suelta", "b", "Suelta", ""),
            nota("n/a.b", "a", "Punto en el nombre", "sin salto final"),
            nota(
                "n/ñandú",
                "b",
                "Ñandú – guion largo · y #",
                "---\nno es frontmatter\n",
            ),
            nota("n/sub/anidada", "a", "  espacios  ", "x\n"),
        ])
        .unwrap();
    let candidato = construir_candidato(&base, &delta).unwrap();
    let estructura = candidato.estructura();
    let archivos: Vec<(String, String)> = estructura
        .nodos()
        .map(|(id, nodo)| {
            (
                format!("{}.md", nodo.path.como_str()),
                serializar_nodo(estructura, id),
            )
        })
        .collect();
    let recargado = parsear_bundle(&archivos, manifiesto).unwrap();
    assert_eq!(recargado.atomos(), estructura.atomos());
    for (id, nodo) in estructura.nodos() {
        let otro = recargado.por_path(&nodo.path).unwrap();
        assert_eq!(
            recargado.anexo(otro),
            estructura.anexo(id),
            "{}",
            nodo.path.como_str()
        );
    }
}
