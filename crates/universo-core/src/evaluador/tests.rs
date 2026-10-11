use super::*;

use crate::estructura::Estructura;
use crate::formula::{Formula, SorteVar, TerminoNodo, TerminoSorte, Var};
use crate::sexpr::parsear;
use crate::tipos::{Etiqueta, Sorte};
use crate::{DeclArista, DeclPredicado, Layout, Manifiesto};
use std::collections::BTreeMap;

struct Mundo {
    estructura: Estructura,
}

fn camino(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn manifiesto_prueba() -> Manifiesto {
    let mut sortes_nodo_disco = BTreeMap::new();
    sortes_nodo_disco.insert(Sorte::nuevo("Patrón"), "pattern".to_string());
    Manifiesto {
        version: "evaluador-test/1".into(),
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
            parent_key: None,
        },
    }
}

fn mundo() -> Mundo {
    let mut estructura = Estructura::nueva(manifiesto_prueba()).unwrap();
    let raiz = estructura
        .insertar_nodo(
            camino("n/root"),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("compartida"),
        )
        .unwrap();
    let medio = estructura
        .insertar_nodo(
            camino("n/middle"),
            Sorte::nuevo("b"),
            Etiqueta::nuevo("compartida"),
        )
        .unwrap();
    let hoja = estructura
        .insertar_nodo(camino("n/leaf"), Sorte::nuevo("a"), Etiqueta::nuevo(""))
        .unwrap();
    estructura
        .insertar_nodo(camino("n/solo"), Sorte::nuevo("b"), Etiqueta::nuevo("otra"))
        .unwrap();
    estructura.asignar_padre(medio, raiz).unwrap();
    estructura.asignar_padre(hoja, medio).unwrap();
    estructura
        .insertar_arista(raiz, medio, Sorte::nuevo("rel"), Etiqueta::nuevo("arista"))
        .unwrap();
    estructura
        .insertar_arista(raiz, medio, Sorte::nuevo("rel"), Etiqueta::nuevo(""))
        .unwrap();
    estructura
        .insertar_arista(medio, hoja, Sorte::nuevo("sub"), Etiqueta::nuevo("scope"))
        .unwrap();
    estructura.asignar_atributo(raiz, "Nivel", "alto").unwrap();
    Mundo { estructura }
}

fn oracion(texto: &str) -> Formula {
    match parsear(texto).unwrap_or_else(|error| panic!("{texto}: {error}")) {
        crate::formula::Sentencia::Oracion(formula) => formula,
        crate::formula::Sentencia::Esquema { .. } => panic!("se esperaba una oración"),
    }
}
#[path = "tests/oracle.rs"]
mod oracle;
#[path = "tests/property.rs"]
mod property;
#[path = "tests/trazabilidad.rs"]
mod trazabilidad;
use oracle::{AsignacionOraculo, oraculo};

fn evaluar_y_comparar(mundo: &Mundo, texto: &str, esperado: bool) {
    let formula = oracion(texto);
    let resultado = evaluar(&formula, &mundo.estructura).unwrap();
    let referencia = oraculo(
        &formula,
        &mundo.estructura,
        &mut AsignacionOraculo::default(),
    )
    .unwrap();
    assert_eq!(resultado, esperado, "{texto}");
    assert_eq!(referencia, esperado, "oráculo: {texto}");
    assert_eq!(resultado, referencia, "diferencia con el oráculo: {texto}");
}

#[test]
fn cada_variante_de_formula_tiene_casos_verdaderos_y_falsos() {
    let mundo = mundo();
    for (texto, esperado) in [
        ("(Type @n/root a)", true),
        ("(Type @n/root b)", false),
        ("(Edge @n/root→n/middle#rel#0 @n/root @n/middle rel)", true),
        ("(Edge @n/root→n/middle#rel#0 @n/solo @n/middle rel)", false),
        ("(Parent @n/middle @n/root)", true),
        ("(Parent @n/root @n/middle)", false),
        ("(HasLab @n/root \"compartida\")", true),
        ("(HasLab @n/root \"otra\")", false),
        ("(HasLab @n/root→n/middle#rel#0 \"arista\")", true),
        ("(HasLab @n/root→n/middle#rel#0 \"scope\")", false),
        ("(Nivel @n/root alto)", true),
        ("(Nivel @n/root bajo)", false),
        ("(tc Parent @n/leaf @n/root)", true),
        ("(tc Parent @n/root @n/leaf)", false),
        ("(tc Parent @n/leaf @n/leaf)", false),
        ("(= @n/root @n/root)", true),
        ("(= @n/root @n/leaf)", false),
        ("(= @n/root→n/middle#rel#0 @n/root→n/middle#rel#0)", true),
        ("(= @n/root→n/middle#rel#0 @n/root→n/middle#rel#1)", false),
        ("(not (Type @n/root b))", true),
        ("(not (Type @n/root a))", false),
        ("(and (Type @n/root a) (Parent @n/middle @n/root))", true),
        ("(and (Type @n/root a) (Parent @n/root @n/middle))", false),
        ("(or (Type @n/root b) (Parent @n/middle @n/root))", true),
        ("(or (Type @n/root b) (Parent @n/root @n/middle))", false),
        ("(=> (Type @n/root a) (Type @n/root b))", false),
        ("(=> (Type @n/root b) (Type @n/root b))", true),
        (
            "(and (exists ((x N)) (Type x a)) (exists ((x N)) (Type x b)))",
            true,
        ),
        ("(forall ((x N)) (or (Type x a) (Type x b)))", true),
        ("(forall ((x N)) (Type x a))", false),
        ("(exists ((x N)) (Type x b))", true),
        ("(exists ((x N)) (Type x Retirado))", false),
        ("(exists ((x N)) (Parent x x))", false),
    ] {
        evaluar_y_comparar(&mundo, texto, esperado);
    }
}

#[test]
fn las_guardas_restringen_el_rango_como_implicacion_y_conjuncion() {
    let mundo = mundo();
    for (texto, esperado) in [
        ("(forall ((x N)) :guarda (Type x a) (Type x a))", true),
        ("(forall ((x N)) :guarda (Type x b) (Type x a))", false),
        ("(exists ((x N)) :guarda (Type x a) (Type x a))", true),
        ("(exists ((x N)) :guarda (Type x b) (Type x a))", false),
        (
            "(exists ((e E) (x N) (y N)) :guarda (Edge e x y rel) (and (= e @n/root→n/middle#rel#1) (= x @n/root)))",
            true,
        ),
        (
            "(forall ((e E) (x N) (y N)) :guarda (Edge e x y rel) (HasLab e \"arista\"))",
            false,
        ),
    ] {
        evaluar_y_comparar(&mundo, texto, esperado);
    }
}

#[test]
fn los_dos_dominios_son_independientes_y_tienen_vacuidad_clasica() {
    let vacia = Estructura::nueva(manifiesto_prueba()).unwrap();
    for (texto, esperado) in [
        ("(forall ((x N)) (= x x))", true),
        ("(exists ((x N)) (= x x))", false),
        ("(forall ((e E)) (= e e))", true),
        ("(exists ((e E)) (= e e))", false),
        ("(forall ((x N) (e E)) (= x x))", true),
        ("(exists ((x N) (e E)) (= x x))", false),
    ] {
        let formula = oracion(texto);
        assert_eq!(evaluar(&formula, &vacia).unwrap(), esperado, "{texto}");
        assert_eq!(
            oraculo(&formula, &vacia, &mut AsignacionOraculo::default()).unwrap(),
            esperado,
            "oráculo: {texto}"
        );
    }

    let mut solo_nodos = Estructura::nueva(manifiesto_prueba()).unwrap();
    solo_nodos
        .insertar_nodo(camino("n/only"), Sorte::nuevo("a"), Etiqueta::nuevo("L"))
        .unwrap();
    for (texto, esperado) in [
        ("(forall ((x N)) (= x x))", true),
        ("(exists ((x N)) (= x x))", true),
        ("(forall ((e E)) (= e e))", true),
        ("(exists ((e E)) (= e e))", false),
    ] {
        let formula = oracion(texto);
        assert_eq!(evaluar(&formula, &solo_nodos).unwrap(), esperado, "{texto}");
    }
}

#[test]
fn conectivos_n_arios_vacios_tienen_identidades_clasicas() {
    let estructura = mundo().estructura;
    assert!(evaluar(&Formula::Y(vec![]), &estructura).unwrap());
    assert!(!evaluar(&Formula::O(vec![]), &estructura).unwrap());
    assert!(
        oraculo(
            &Formula::Y(vec![]),
            &estructura,
            &mut AsignacionOraculo::default()
        )
        .unwrap()
    );
    assert!(
        !oraculo(
            &Formula::O(vec![]),
            &estructura,
            &mut AsignacionOraculo::default()
        )
        .unwrap()
    );
}

#[test]
fn valida_toda_la_formula_antes_de_cortocircuitar() {
    let mundo = mundo();
    let formula = oracion("(or (Type @n/root a) (Type @n/missing a))");
    assert_eq!(
        evaluar(&formula, &mundo.estructura),
        Err(ErrorEvaluacion::ConstanteNodoInexistente(camino(
            "n/missing"
        )))
    );
}

#[test]
fn limita_profundidad_de_ast_directo_sin_desbordar_la_pila() {
    let estructura = mundo().estructura;
    let anidar = |cuantas| {
        (0..cuantas).fold(Formula::Y(vec![]), |interior, _| {
            Formula::No(Box::new(interior))
        })
    };

    assert_eq!(
        evaluar(&anidar(MAX_PROFUNDIDAD_AST - 1), &estructura),
        Ok(false)
    );
    assert_eq!(
        evaluar(&anidar(MAX_PROFUNDIDAD_AST), &estructura),
        Err(ErrorEvaluacion::ProfundidadAstExcedida {
            maximo: MAX_PROFUNDIDAD_AST,
        })
    );
}

#[test]
fn limita_ligaduras_por_cuantificador_antes_de_enumerar() {
    let estructura = mundo().estructura;
    let formula_con_ligaduras = |cantidad| {
        let ligaduras = (0..cantidad)
            .map(|indice| (Var::nuevo(&format!("x{indice}")), SorteVar::Nodo))
            .collect();
        Formula::Existe {
            ligaduras,
            guarda: None,
            cuerpo: Box::new(Formula::Type {
                sujeto: TerminoNodo::Var(Var::nuevo("x0")),
                sorte: TerminoSorte::Literal(Sorte::nuevo("a")),
            }),
        }
    };

    assert_eq!(
        evaluar(
            &formula_con_ligaduras(MAX_LIGADURAS_POR_CUANTIFICADOR),
            &estructura
        ),
        Ok(true)
    );
    assert_eq!(
        evaluar(
            &formula_con_ligaduras(MAX_LIGADURAS_POR_CUANTIFICADOR + 1),
            &estructura
        ),
        Err(ErrorEvaluacion::DemasiadasLigaduras {
            cantidad: MAX_LIGADURAS_POR_CUANTIFICADOR + 1,
            maximo: MAX_LIGADURAS_POR_CUANTIFICADOR,
        })
    );
}

#[test]
fn cuantificadores_anidados_al_limite_no_acumulan_pila_por_ligaduras() {
    let estructura = mundo().estructura;
    let mut formula = Formula::Type {
        sujeto: TerminoNodo::Var(Var::nuevo(&format!("x{}_0", MAX_PROFUNDIDAD_AST - 2))),
        sorte: TerminoSorte::Literal(Sorte::nuevo("a")),
    };
    for nivel in (0..MAX_PROFUNDIDAD_AST - 1).rev() {
        let ligaduras = (0..MAX_LIGADURAS_POR_CUANTIFICADOR)
            .map(|indice| (Var::nuevo(&format!("x{nivel}_{indice}")), SorteVar::Nodo))
            .collect();
        formula = Formula::Existe {
            ligaduras,
            guarda: None,
            cuerpo: Box::new(formula),
        };
    }

    assert_eq!(evaluar(&formula, &estructura), Ok(true));
}

#[test]
fn presupuesto_cubre_preflight_y_visitas_de_formula() {
    let estructura = mundo().estructura;
    let formula = Formula::Y(vec![]);
    assert_eq!(
        evaluar_con_presupuesto(&formula, &estructura, 0),
        Err(ErrorEvaluacion::PresupuestoAgotado { limite: 0 })
    );
    assert_eq!(
        evaluar_con_presupuesto(&formula, &estructura, 1),
        Err(ErrorEvaluacion::PresupuestoAgotado { limite: 1 })
    );
    assert_eq!(evaluar_con_presupuesto(&formula, &estructura, 2), Ok(true));
    assert_eq!(
        evaluar_con_presupuesto(&formula, &estructura, PRESUPUESTO_EVALUACION_POR_DEFECTO),
        evaluar(&formula, &estructura)
    );

    let ancha = Formula::Y(vec![Formula::Y(vec![]); 32]);
    assert_eq!(
        evaluar_con_presupuesto(&ancha, &estructura, 16),
        Err(ErrorEvaluacion::PresupuestoAgotado { limite: 16 })
    );
}

#[test]
fn presupuesto_cuenta_tuplas_y_pasos_de_clausura_transitiva() {
    let estructura = mundo().estructura;
    let x = Var::nuevo("x");
    let producto = Formula::ParaTodo {
        ligaduras: vec![
            (x.clone(), SorteVar::Nodo),
            (Var::nuevo("y"), SorteVar::Nodo),
            (Var::nuevo("z"), SorteVar::Nodo),
        ],
        guarda: None,
        cuerpo: Box::new(Formula::IgualNodo(
            TerminoNodo::Var(x.clone()),
            TerminoNodo::Var(x),
        )),
    };
    assert_eq!(
        evaluar_con_presupuesto(&producto, &estructura, 30),
        Err(ErrorEvaluacion::PresupuestoAgotado { limite: 30 })
    );
    assert_eq!(
        evaluar_con_presupuesto(&producto, &estructura, 400),
        Ok(true)
    );

    let tc = oracion("(tc Parent @n/leaf @n/root)");
    assert_eq!(
        evaluar_con_presupuesto(&tc, &estructura, 3),
        Err(ErrorEvaluacion::PresupuestoAgotado { limite: 3 })
    );
    assert_eq!(evaluar_con_presupuesto(&tc, &estructura, 4), Ok(true));
}

#[test]
fn constantes_y_simbolos_fuera_de_la_firma_devuelven_error_tipado() {
    let mundo = mundo();
    let casos = [
        (
            "(= @n/missing @n/root)",
            ErrorEvaluacion::ConstanteNodoInexistente(camino("n/missing")),
        ),
        (
            "(= @n/root→n/middle#rel#99 @n/root→n/middle#rel#0)",
            ErrorEvaluacion::ConstanteAristaInexistente(
                IdArista::parsear("n/root→n/middle#rel#99").unwrap(),
            ),
        ),
        (
            "(Type @n/root fantasma)",
            ErrorEvaluacion::SorteNodoNoDeclarada(Sorte::nuevo("fantasma")),
        ),
        (
            "(Edge @n/root→n/middle#rel#0 @n/root @n/middle fantasma)",
            ErrorEvaluacion::SorteAristaNoDeclarada(Sorte::nuevo("fantasma")),
        ),
        (
            "(HasLab @n/root \"no-usada\")",
            ErrorEvaluacion::EtiquetaNoEnUso(Etiqueta::nuevo("no-usada")),
        ),
        (
            "(Ausente @n/root x)",
            ErrorEvaluacion::PredicadoNoDeclarado("Ausente".into()),
        ),
        (
            "(Nivel @n/root medio)",
            ErrorEvaluacion::ConstanteFueraDeDominio {
                predicado: "Nivel".into(),
                constante: "medio".into(),
            },
        ),
    ];
    for (texto, esperado) in casos {
        assert_eq!(
            evaluar(&oracion(texto), &mundo.estructura),
            Err(esperado),
            "{texto}"
        );
    }
}

#[test]
fn una_formula_abierta_o_con_metavariable_no_instanciada_es_error() {
    let mundo = mundo();
    let abierta = Formula::Parent {
        hijo: TerminoNodo::Var(Var::nuevo("x")),
        padre: TerminoNodo::Constante(camino("n/root")),
    };
    assert_eq!(
        evaluar(&abierta, &mundo.estructura),
        Err(ErrorEvaluacion::VariableNoLigada {
            variable: Var::nuevo("x"),
            sorte: SorteVar::Nodo,
        })
    );

    let con_meta = Formula::Type {
        sujeto: TerminoNodo::Constante(camino("n/root")),
        sorte: TerminoSorte::Meta(Var::nuevo("k")),
    };
    assert_eq!(
        evaluar(&con_meta, &mundo.estructura),
        Err(ErrorEvaluacion::MetaNoInstanciada(Var::nuevo("k")))
    );
}

#[test]
fn un_ast_mal_tipado_no_llega_a_la_evaluacion() {
    let mundo = mundo();
    let mal_tipada = Formula::ParaTodo {
        ligaduras: vec![(Var::nuevo("e"), SorteVar::Arista)],
        guarda: None,
        cuerpo: Box::new(Formula::Parent {
            hijo: TerminoNodo::Var(Var::nuevo("e")),
            padre: TerminoNodo::Constante(camino("n/root")),
        }),
    };
    assert_eq!(
        evaluar(&mal_tipada, &mundo.estructura),
        Err(ErrorEvaluacion::VariableDeSorteIncorrecta {
            variable: Var::nuevo("e"),
            esperada: SorteVar::Nodo,
        })
    );

    let variable = Var::nuevo("x");
    let redeclarada = Formula::ParaTodo {
        ligaduras: vec![(variable.clone(), SorteVar::Nodo)],
        guarda: None,
        cuerpo: Box::new(Formula::Existe {
            ligaduras: vec![(variable.clone(), SorteVar::Nodo)],
            guarda: None,
            cuerpo: Box::new(Formula::IgualNodo(
                TerminoNodo::Var(variable.clone()),
                TerminoNodo::Var(variable.clone()),
            )),
        }),
    };
    assert_eq!(
        evaluar(&redeclarada, &mundo.estructura),
        Err(ErrorEvaluacion::VariableRedeclarada(variable))
    );
}
