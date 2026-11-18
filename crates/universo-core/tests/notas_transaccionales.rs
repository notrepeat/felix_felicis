//! Crear notas (nodos) por delta: candidato, validaciones y padre legado.
use universo_core::sexpr::parsear_programa;
use universo_core::{
    Atomo, BundlePath, ClaseOperacion, Delta, ErrorCandidato, ErrorDelta, Estructura, Etiqueta,
    NotaNueva, ResultadoPropuesta, Sorte, Sujeto, construir_candidato, proponer,
};

fn p(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn base() -> Estructura {
    let mut estructura = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    estructura
        .insertar_nodo(
            p("n/existente"),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("Existente"),
        )
        .unwrap();
    estructura
}

fn nota(path: &str, tipo: &str, titulo: &str) -> NotaNueva {
    NotaNueva {
        path: p(path),
        tipo: Sorte::nuevo(tipo),
        etiqueta: Etiqueta::nuevo(titulo),
        cuerpo: "Cuerpo de la nota.\n".to_string(),
    }
}

fn crear(notas: Vec<NotaNueva>, add: Vec<Atomo>) -> Delta {
    Delta::nuevo(add, []).unwrap().con_notas(notas).unwrap()
}

fn codigo(error: ErrorCandidato) -> &'static str {
    assert_eq!(error.clase(), Some(ClaseOperacion::Nota));
    error.codigo()
}

#[test]
fn crear_una_nota_la_agrega_al_candidato_sin_tocar_el_snapshot() {
    let snapshot = base();
    let delta = crear(vec![nota("n/nueva", "b", "Nueva")], vec![]);
    let candidato = construir_candidato(&snapshot, &delta).unwrap();
    let estructura = candidato.estructura();
    let id = estructura.por_path(&p("n/nueva")).unwrap();
    let nodo = estructura.nodo(id).unwrap();
    assert_eq!(nodo.tipo, Sorte::nuevo("b"));
    assert_eq!(nodo.etiqueta, Etiqueta::nuevo("Nueva"));
    assert_eq!(estructura.anexo(id).unwrap().cuerpo, "Cuerpo de la nota.\n");
    assert!(estructura.anexo(id).unwrap().passthrough.is_empty());
    assert_eq!(estructura.len_nodos(), 2);
    assert!(snapshot.por_path(&p("n/nueva")).is_none());
}

#[test]
fn el_resto_del_delta_puede_referirse_a_la_nota_nueva() {
    let snapshot = base();
    let atributo = Atomo::Ext {
        sujeto: p("n/nueva"),
        predicado: "Nivel".into(),
        constante: "alto".into(),
    };
    let delta = crear(vec![nota("n/nueva", "b", "Nueva")], vec![atributo.clone()]);
    let candidato = construir_candidato(&snapshot, &delta).unwrap();
    assert!(candidato.estructura().atomos().contains(&atributo));
}

#[test]
fn las_invariantes_se_evaluan_con_la_nota_creada() {
    let fuentes = parsear_programa("(forall ((x N)) (Type x a))").unwrap();
    let rechazo = crear(vec![nota("n/nueva", "b", "Nueva")], vec![]);
    assert!(matches!(
        proponer(&base(), &rechazo, &fuentes).unwrap(),
        ResultadoPropuesta::RechazadaInvariantes(_)
    ));
    let admision = crear(vec![nota("n/nueva", "a", "Nueva")], vec![]);
    assert!(matches!(
        proponer(&base(), &admision, &fuentes).unwrap(),
        ResultadoPropuesta::Admitida(_)
    ));
}

#[test]
fn una_nota_bajo_un_index_existente_recibe_el_padre_legado() {
    let mut snapshot = base();
    let index = snapshot
        .insertar_nodo(p("n/index"), Sorte::nuevo("a"), Etiqueta::nuevo("Índice"))
        .unwrap();
    let delta = crear(
        vec![
            nota("n/hija", "a", "Hija"),
            nota("n/libre/suelta", "a", "Suelta"),
        ],
        vec![],
    );
    let candidato = construir_candidato(&snapshot, &delta).unwrap();
    let estructura = candidato.estructura();
    let hija = estructura.por_path(&p("n/hija")).unwrap();
    assert_eq!(estructura.padre(hija), Some(index));
    // n/libre está en sin_iota: sin padre implícito.
    let suelta = estructura.por_path(&p("n/libre/suelta")).unwrap();
    assert_eq!(estructura.padre(suelta), None);
}

#[test]
fn validaciones_de_la_nota_nueva() {
    let snapshot = base();
    let casos = [
        (nota("n/existente", "a", "Otra"), "note_exists"),
        (nota("n/EXISTENTE", "a", "Otra"), "note_exists"),
        (nota("n/index", "a", "Índice"), "note_index_not_supported"),
        (
            nota("n/sub/index", "a", "Índice"),
            "note_index_not_supported",
        ),
        (nota("n/lapida", "Retirado", "Lápida"), "note_reserved_sort"),
        (nota("n/patron", "Patrón", "Patrón"), "note_reserved_sort"),
        (nota("otra/fuera", "a", "Fuera"), "note_outside_layout"),
        (nota("n/x", "desconocido", "X"), "note_unknown_sort"),
        (nota("log", "a", "Sin directorio"), "note_outside_layout"),
        // `ignorados = ["log.md"]`: el cargador ignoraría `n/log.md`.
        (nota("n/log", "a", "Ignorada"), "note_outside_layout"),
    ];
    for (nota, esperado) in casos {
        let delta = crear(vec![nota], vec![]);
        let error = construir_candidato(&snapshot, &delta).unwrap_err();
        assert_eq!(codigo(error), esperado);
    }
    let mut con_retorno = nota("n/x", "a", "X");
    con_retorno.cuerpo = "línea\r\n".into();
    let error = construir_candidato(&snapshot, &crear(vec![con_retorno], vec![])).unwrap_err();
    assert_eq!(codigo(error), "note_invalid_body");
    let etiqueta_mala = nota("n/x", "a", "dos\nlíneas");
    let error = construir_candidato(&snapshot, &crear(vec![etiqueta_mala], vec![])).unwrap_err();
    assert_eq!(codigo(error), "note_invalid_label");
}

#[test]
fn notas_repetidas_en_un_delta_se_rechazan_al_construirlo() {
    let error = Delta::default()
        .con_notas(vec![nota("n/x", "a", "X"), nota("n/X", "b", "Y")])
        .unwrap_err();
    assert!(matches!(error, ErrorDelta::NotaDuplicada { .. }));
    let ordenado = Delta::default()
        .con_notas(vec![nota("n/z", "a", "Z"), nota("n/a", "a", "A")])
        .unwrap();
    let paths: Vec<_> = ordenado.crear().iter().map(|n| n.path.como_str()).collect();
    assert_eq!(paths, vec!["n/a", "n/z"]);
    // También entre llamadas sucesivas.
    let error = ordenado
        .con_notas(vec![nota("n/A", "a", "Otra")])
        .unwrap_err();
    assert!(matches!(error, ErrorDelta::NotaDuplicada { .. }));
}

#[test]
fn un_delta_sin_notas_se_comporta_igual_que_antes() {
    let snapshot = base();
    let candidato = construir_candidato(&snapshot, &Delta::default()).unwrap();
    assert_eq!(candidato.estructura().atomos(), snapshot.atomos());
    let _ = Sujeto::Nodo(p("n/existente"));
}
