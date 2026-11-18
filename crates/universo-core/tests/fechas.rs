//! C2: predicados de tipo fecha (ISO `AAAA-MM-DD`, dominio abierto).
use universo_core::invariante::{CodigoValidacionInvariante, validar_sentencia_fuente};
use universo_core::manifiesto::DeclPredicado;
use universo_core::sexpr::{parsear, parsear_programa};
use universo_core::{BundlePath, Estructura, Etiqueta, Manifiesto, Sentencia, Sorte, evaluar};

fn manifiesto() -> Manifiesto {
    let mut m = universo_testkit::manifiesto_minimo();
    m.predicados.push(DeclPredicado {
        nombre: "Vigencia".into(),
        clave: "vigente_hasta".into(),
        dominio: Default::default(),
        orden: Vec::new(),
        multivalor: false,
        fecha: true,
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
        e.asignar_atributo(id, "Vigencia", v).unwrap();
    }
    e
}

fn vale(texto: &str, valor: Option<&str>) -> bool {
    let Sentencia::Oracion(f) = parsear(texto).unwrap() else {
        panic!()
    };
    evaluar(&f, &estructura(valor)).unwrap()
}

#[test]
fn compara_fechas_cronologicamente_y_acepta_cualquier_fecha_valida() {
    assert!(vale("(>= Vigencia @n/x 2027-01-01)", Some("2027-06-30")));
    assert!(!vale("(>= Vigencia @n/x 2027-01-01)", Some("2026-12-31")));
    assert!(vale("(< Vigencia @n/x 2030-01-01)", Some("2029-12-31")));
    assert!(vale("(Vigencia @n/x 2028-02-29)", Some("2028-02-29")));
    assert!(!vale("(>= Vigencia @n/x 2000-01-01)", None));
}

#[test]
fn solo_se_admiten_fechas_iso_validas() {
    let mut e = estructura(None);
    let id = e.por_path(&BundlePath::nuevo("n/x").unwrap()).unwrap();
    for malo in [
        "2027-13-01",
        "2027-02-30",
        "2027-2-1",
        "hoy",
        "2027-01-01T00:00",
        "2027-02-29",
    ] {
        assert!(e.asignar_atributo(id, "Vigencia", malo).is_err(), "{malo}");
    }
    assert!(e.asignar_atributo(id, "Vigencia", "2024-02-29").is_ok());
}

#[test]
fn validacion_de_reglas_y_manifiesto_con_fechas() {
    let e = estructura(None);
    let codigos = |texto: &str| -> Vec<CodigoValidacionInvariante> {
        let fuentes = parsear_programa(texto).unwrap();
        validar_sentencia_fuente(&fuentes[0], e.manifiesto(), &e)
            .into_iter()
            .map(|error| error.codigo)
            .collect()
    };
    assert!(codigos("(>= Vigencia @n/x 2027-01-01)").is_empty());
    assert_eq!(
        codigos("(>= Vigencia @n/x mañana)"),
        vec![CodigoValidacionInvariante::ConstanteExtFueraDeDominio]
    );
    // Un esquema no puede enumerar un dominio abierto.
    assert!(
        codigos("(esquema ((d Dom.Vigencia)) (Vigencia @n/x d))")
            .contains(&CodigoValidacionInvariante::MetaDominioIncompatible)
    );
    let mut m = manifiesto();
    m.predicados
        .last_mut()
        .unwrap()
        .dominio
        .insert("2027-01-01".into());
    assert!(m.validar().is_err(), "una fecha no declara dominio");
    let mut m = manifiesto();
    m.predicados.last_mut().unwrap().multivalor = true;
    assert!(m.validar().is_err(), "fecha y multivalor son incompatibles");
    assert!(manifiesto().validar().is_ok());
}
