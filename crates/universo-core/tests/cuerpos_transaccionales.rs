//! Actualizar el cuerpo de notas existentes por delta (anexo opaco).
use universo_core::anexo::Entrada;
use universo_core::{
    Anexo, BundlePath, ClaseOperacion, CuerpoNuevo, Delta, ErrorDelta, Estructura, Etiqueta,
    NotaNueva, Sorte, construir_candidato,
};

fn p(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn base() -> Estructura {
    let mut estructura = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let id = estructura
        .insertar_nodo(p("n/nota"), Sorte::nuevo("a"), Etiqueta::nuevo("Nota"))
        .unwrap();
    estructura
        .anexar(
            id,
            Anexo {
                cuerpo: "viejo\n".into(),
                passthrough: vec![Entrada {
                    clave: "extra".into(),
                    crudo: "extra: 1\n".into(),
                }],
            },
        )
        .unwrap();
    estructura
}

fn cuerpo(path: &str, texto: &str) -> CuerpoNuevo {
    CuerpoNuevo {
        path: p(path),
        cuerpo: texto.into(),
    }
}

#[test]
fn reemplaza_el_cuerpo_y_conserva_el_frontmatter_no_mapeado() {
    let snapshot = base();
    let delta = Delta::default()
        .con_cuerpos([cuerpo("n/nota", "nuevo\n")])
        .unwrap();
    let candidato = construir_candidato(&snapshot, &delta).unwrap();
    let id = candidato.estructura().por_path(&p("n/nota")).unwrap();
    let anexo = candidato.estructura().anexo(id).unwrap();
    assert_eq!(anexo.cuerpo, "nuevo\n");
    assert_eq!(anexo.passthrough.len(), 1);
    assert_eq!(candidato.estructura().atomos(), snapshot.atomos());
    let original = snapshot.por_path(&p("n/nota")).unwrap();
    assert_eq!(snapshot.anexo(original).unwrap().cuerpo, "viejo\n");
}

#[test]
fn errores_de_actualizacion() {
    let snapshot = base();
    let casos = [
        (
            Delta::default()
                .con_cuerpos([cuerpo("n/no-existe", "x")])
                .unwrap(),
            "note_not_found",
        ),
        (
            Delta::default()
                .con_cuerpos([cuerpo("n/nota", "a\r\nb")])
                .unwrap(),
            "note_invalid_body",
        ),
        (
            Delta::default()
                .con_notas([NotaNueva {
                    path: p("n/nueva"),
                    tipo: Sorte::nuevo("a"),
                    etiqueta: Etiqueta::nuevo("Nueva"),
                    cuerpo: String::new(),
                }])
                .unwrap()
                .con_cuerpos([cuerpo("n/nueva", "x")])
                .unwrap(),
            "note_update_of_created",
        ),
    ];
    for (delta, esperado) in casos {
        let error = construir_candidato(&snapshot, &delta).unwrap_err();
        assert_eq!(error.clase(), Some(ClaseOperacion::Nota));
        assert_eq!(error.codigo(), esperado);
    }
    let duplicado = Delta::default().con_cuerpos([cuerpo("n/nota", "a"), cuerpo("n/nota", "b")]);
    assert!(matches!(duplicado, Err(ErrorDelta::CuerpoDuplicado { .. })));
}
