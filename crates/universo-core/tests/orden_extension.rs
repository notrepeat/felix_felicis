//! C1: predicados con dominio ordenado y comparaciones `<`, `<=`, `>`, `>=`.
use universo_core::invariante::{CodigoValidacionInvariante, validar_sentencia_fuente};
use universo_core::manifiesto::DeclPredicado;
use universo_core::sexpr::{parsear, parsear_programa};
use universo_core::{
    BundlePath, Estructura, Etiqueta, Formula, Manifiesto, Sentencia, Sorte, evaluar,
};

fn manifiesto() -> Manifiesto {
    let mut m = universo_testkit::manifiesto_minimo();
    m.predicados.push(DeclPredicado {
        nombre: "Confianza".into(),
        clave: "confianza".into(),
        dominio: ["baja", "media", "alta"]
            .into_iter()
            .map(String::from)
            .collect(),
        orden: vec!["baja".into(), "media".into(), "alta".into()],
    });
    m
}

fn estructura(valor: Option<&str>) -> Estructura {
    let mut e = Estructura::nueva(manifiesto()).unwrap();
    let id = e
        .insertar_nodo(
            BundlePath::nuevo("n/x").unwrap(),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("X"),
        )
        .unwrap();
    if let Some(v) = valor {
        e.asignar_atributo(id, "Confianza", v).unwrap();
    }
    e
}

fn vale(texto: &str, valor: Option<&str>) -> bool {
    let Sentencia::Oracion(f) = parsear(texto).unwrap() else {
        panic!("oración");
    };
    evaluar(&f, &estructura(valor)).unwrap()
}

#[test]
fn compara_segun_el_orden_declarado_no_el_alfabetico() {
    // Alfabéticamente "alta" < "baja" < "media"; el orden declarado manda.
    assert!(vale("(>= Confianza @n/x media)", Some("alta")));
    assert!(vale("(>= Confianza @n/x media)", Some("media")));
    assert!(!vale("(>= Confianza @n/x media)", Some("baja")));
    assert!(vale("(< Confianza @n/x media)", Some("baja")));
    assert!(!vale("(> Confianza @n/x alta)", Some("alta")));
    assert!(vale("(<= Confianza @n/x alta)", Some("alta")));
    // Sin valor, ninguna comparación es verdadera.
    assert!(!vale("(>= Confianza @n/x baja)", None));
    assert!(!vale("(< Confianza @n/x alta)", None));
}

#[test]
fn forma_canonica_ida_y_vuelta() {
    let s = parsear("(forall ((x N)) (>= Confianza x media))").unwrap();
    assert!(matches!(&s, Sentencia::Oracion(Formula::ParaTodo { .. })));
    assert_eq!(parsear(&s.to_string()).unwrap(), s);
    assert_eq!(s.to_string(), "(forall ((x N)) (>= Confianza x media))");
}

#[test]
fn comparar_un_predicado_sin_orden_o_fuera_de_dominio_es_error_de_validacion() {
    let e = estructura(Some("alta"));
    let codigos = |texto: &str| -> Vec<CodigoValidacionInvariante> {
        let fuentes = parsear_programa(texto).unwrap();
        validar_sentencia_fuente(&fuentes[0], e.manifiesto(), &e)
            .into_iter()
            .map(|error| error.codigo)
            .collect()
    };
    assert!(codigos("(>= Confianza @n/x media)").is_empty());
    assert_eq!(
        codigos("(>= Nivel @n/x alto)"),
        vec![CodigoValidacionInvariante::PredicadoNoOrdenado]
    );
    assert_eq!(
        codigos("(>= Confianza @n/x altisima)"),
        vec![CodigoValidacionInvariante::ConstanteExtFueraDeDominio]
    );
    assert_eq!(
        codigos("(>= Nadie @n/x a)"),
        vec![CodigoValidacionInvariante::PredicadoNoDeclarado]
    );
}

#[test]
fn el_orden_debe_ser_una_permutacion_del_dominio() {
    let mut m = manifiesto();
    m.predicados.last_mut().unwrap().orden = vec!["baja".into(), "alta".into()];
    assert!(m.validar().is_err());
    let mut m = manifiesto();
    m.predicados.last_mut().unwrap().orden = vec![];
    assert!(m.validar().is_ok(), "sin orden es válido (no comparable)");
}

#[test]
fn la_negacion_de_una_comparacion_es_verdadera_sin_valor() {
    // `(not (>= P x c))` no equivale a `(< P x c)` cuando falta el valor.
    assert!(vale("(not (>= Confianza @n/x media))", None));
    assert!(!vale("(< Confianza @n/x media)", None));
}

#[test]
fn esquemas_y_errores_de_autoria_de_comparaciones() {
    let e = estructura(Some("alta"));
    let codigos = |texto: &str| -> Vec<CodigoValidacionInvariante> {
        let fuentes = parsear_programa(texto).unwrap();
        validar_sentencia_fuente(&fuentes[0], e.manifiesto(), &e)
            .into_iter()
            .map(|error| error.codigo)
            .collect()
    };
    // Un esquema sobre el dominio ordenado vale; sobre uno no ordenado, no.
    assert!(codigos("(esquema ((c Dom.Confianza)) (>= Confianza @n/x c))").is_empty());
    assert!(
        codigos("(esquema ((c Dom.Nivel)) (>= Nivel @n/x c))")
            .contains(&CodigoValidacionInvariante::PredicadoNoOrdenado)
    );
    for malo in ["(>= Confianza @n/x)", "(>= (Type @n/x a) @n/x media)"] {
        assert!(parsear_programa(malo).is_err(), "{malo}");
    }
    let mut m = manifiesto();
    m.predicados.push(DeclPredicado {
        nombre: ">=".into(),
        clave: "mayor".into(),
        dominio: ["x"].into_iter().map(String::from).collect(),
        orden: Vec::new(),
    });
    assert!(
        m.validar().is_err(),
        "los símbolos de comparación están reservados"
    );
}
