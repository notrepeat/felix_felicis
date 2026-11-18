//! C3: atributos multivalor (`multivalor = true`).
use universo_core::manifiesto::DeclPredicado;
use universo_core::sexpr::parsear;
use universo_core::{
    Atomo, BundlePath, Delta, Estructura, Etiqueta, Manifiesto, Sentencia, Sorte,
    construir_candidato, evaluar,
};

fn manifiesto() -> Manifiesto {
    let mut m = universo_testkit::manifiesto_minimo();
    m.predicados.push(DeclPredicado {
        nombre: "Tema".into(),
        clave: "temas".into(),
        dominio: ["cache", "red", "seguridad"]
            .into_iter()
            .map(String::from)
            .collect(),
        orden: Vec::new(),
        multivalor: true,
        fecha: false,
    });
    m
}

fn p(t: &str) -> BundlePath {
    BundlePath::nuevo(t).unwrap()
}

fn base() -> Estructura {
    let mut e = Estructura::nueva(manifiesto()).unwrap();
    let id = e
        .insertar_nodo(p("n/x"), Sorte::nuevo("a"), Etiqueta::nuevo("X"))
        .unwrap();
    e.asignar_atributo(id, "Tema", "cache").unwrap();
    e.asignar_atributo(id, "Tema", "red").unwrap();
    e
}

fn tema(valor: &str) -> Atomo {
    Atomo::Ext {
        predicado: "Tema".into(),
        sujeto: p("n/x"),
        constante: valor.into(),
    }
}

fn vale(e: &Estructura, texto: &str) -> bool {
    let Sentencia::Oracion(f) = parsear(texto).unwrap() else {
        panic!()
    };
    evaluar(&f, e).unwrap()
}

#[test]
fn un_nodo_tiene_varios_valores_y_cada_uno_es_un_atomo() {
    let e = base();
    let atomos = e.atomos();
    assert!(atomos.contains(&tema("cache")) && atomos.contains(&tema("red")));
    assert!(!atomos.contains(&tema("seguridad")));
    assert!(vale(&e, "(Tema @n/x cache)") && vale(&e, "(Tema @n/x red)"));
    assert!(!vale(&e, "(Tema @n/x seguridad)"));
    // El predicado funcional sigue siendo funcional.
    let id = e.por_path(&p("n/x")).unwrap();
    let mut f = base();
    f.asignar_atributo(id, "Nivel", "bajo").unwrap();
    f.asignar_atributo(id, "Nivel", "alto").unwrap();
    assert_eq!(
        f.atributos(id).get("Nivel").map(String::as_str),
        Some("alto")
    );
}

#[test]
fn deltas_agregan_y_retiran_valores_sin_conflicto_funcional() {
    let snapshot = base();
    let delta = Delta::nuevo([tema("seguridad"), tema("cache")], [tema("red")]).unwrap();
    let candidato = construir_candidato(&snapshot, &delta).unwrap();
    let atomos = candidato.estructura().atomos();
    assert!(atomos.contains(&tema("cache")) && atomos.contains(&tema("seguridad")));
    assert!(!atomos.contains(&tema("red")));
    let ausente = Delta::nuevo([], [tema("seguridad")]).unwrap();
    assert_eq!(
        construir_candidato(&snapshot, &ausente)
            .unwrap_err()
            .codigo(),
        "remove_not_found"
    );
}

#[test]
fn orden_y_multivalor_son_incompatibles() {
    let mut m = manifiesto();
    let ultimo = m.predicados.last_mut().unwrap();
    ultimo.orden = ultimo.dominio.iter().cloned().collect();
    assert!(m.validar().is_err());
}
