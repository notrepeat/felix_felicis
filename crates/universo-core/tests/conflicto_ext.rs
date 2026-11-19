//! T4: un conflicto funcional de atributo es un rechazo con clase `Ext`, no un error técnico.
use universo_core::{
    Atomo, BundlePath, ClaseConflicto, Delta, Estructura, Etiqueta, ResultadoPropuesta, Sorte,
    proponer,
};

fn base() -> Estructura {
    let mut m = universo_testkit::manifiesto_minimo();
    m.predicados.push(universo_core::DeclPredicado {
        nombre: "Tema".into(),
        clave: "temas".into(),
        dominio: ["cache", "red"].into_iter().map(String::from).collect(),
        orden: Vec::new(),
        multivalor: true,
        fecha: false,
    });
    let mut e = Estructura::nueva(m).unwrap();
    let id = e
        .insertar_nodo(
            BundlePath::nuevo("n/x").unwrap(),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("X"),
        )
        .unwrap();
    e.asignar_atributo(id, "Nivel", "bajo").unwrap();
    e.asignar_atributo(id, "Tema", "cache").unwrap();
    e
}

fn ext(predicado: &str, valor: &str) -> Atomo {
    Atomo::Ext {
        predicado: predicado.into(),
        sujeto: BundlePath::nuevo("n/x").unwrap(),
        constante: valor.into(),
    }
}

#[test]
fn reemplazar_sin_retirar_es_un_conflicto_ext() {
    let delta = Delta::nuevo([ext("Nivel", "alto")], []).unwrap();
    let ResultadoPropuesta::RechazadaConflictos(conflictos) =
        proponer(&base(), &delta, &[]).unwrap()
    else {
        panic!("se esperaba un rechazo por conflicto")
    };
    let clases: Vec<_> = conflictos
        .diagnosticos()
        .iter()
        .map(|d| (d.clase(), d.cantidad()))
        .collect();
    assert_eq!(clases, vec![(ClaseConflicto::Ext, 1)]);
    assert_eq!(ClaseConflicto::Ext.to_string(), "Ext");
}

#[test]
fn dos_valores_nuevos_del_mismo_atributo_tambien() {
    let delta = Delta::nuevo([ext("Nivel", "alto")], [ext("Nivel", "bajo")]).unwrap();
    assert!(matches!(
        proponer(&base(), &delta, &[]).unwrap(),
        ResultadoPropuesta::Admitida(_)
    ));
    let mut agregar = vec![ext("Nivel", "alto")];
    agregar.push(Atomo::Ext {
        predicado: "Nivel".into(),
        sujeto: BundlePath::nuevo("n/x").unwrap(),
        constante: "bajo".into(),
    });
    let delta = Delta::nuevo(agregar, []).unwrap();
    assert!(matches!(
        proponer(&base(), &delta, &[]).unwrap(),
        ResultadoPropuesta::RechazadaConflictos(_)
    ));
}

#[test]
fn un_atributo_multivalor_no_es_funcional() {
    let delta = Delta::nuevo([ext("Tema", "red")], []).unwrap();
    assert!(matches!(
        proponer(&base(), &delta, &[]).unwrap(),
        ResultadoPropuesta::Admitida(_)
    ));
}
