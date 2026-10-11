use super::*;

use crate::evaluador::{ReglaEvidencia, TipoCuantificador, evaluar_trazable};
use crate::formula::Sentencia;
use crate::invariante::expansion::{MetricasExpansion, OracionExpandida, ResultadoExpansionFuente};
use crate::manifiesto::tests::base;
use crate::sexpr::parsear;
use crate::veredicto::{ModoExplicacion, ResultadoExplicaciones, explicar};

fn ruta(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn estructura_con_hoja() -> Estructura {
    let mut estructura = Estructura::nueva(base()).unwrap();
    let raiz = estructura
        .insertar_nodo(
            ruta("n/root"),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("root-title"),
        )
        .unwrap();
    let hoja = estructura
        .insertar_nodo(
            ruta("n/leaf"),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("synthetic-label"),
        )
        .unwrap();
    estructura.asignar_padre(hoja, raiz).unwrap();
    estructura.asignar_atributo(hoja, "Nivel", "alto").unwrap();
    estructura
        .insertar_arista(
            raiz,
            hoja,
            Sorte::nuevo("rel"),
            Etiqueta::nuevo("edge-label"),
        )
        .unwrap();
    estructura
}

fn formula(texto: &str) -> Formula {
    match parsear(texto).unwrap() {
        Sentencia::Oracion(formula) => formula,
        Sentencia::Esquema { .. } => panic!("se esperaba una oración"),
    }
}

fn instancia_falsa(formula: &Formula, estructura: &Estructura) -> InstanciaCuantificada {
    let traza = evaluar_trazable(formula, estructura).unwrap();
    assert!(!traza.valor);
    let ReglaEvidencia::Cuantificador {
        tipo: TipoCuantificador::ParaTodo,
        instancias,
        ..
    } = traza.evidencia.regla
    else {
        panic!("la raíz de evidencia no era un para-todo");
    };
    instancias
        .into_iter()
        .find(|instancia| !instancia.valor)
        .expect("se esperaba una asignación universal falsificante")
}

fn parsear_hecho(texto: &str) -> Atomo {
    Atomo::parsear(texto).unwrap()
}

fn condicion_de_falsedad_satisfecha(requeridos: &[Atomo], subconjunto: &[Atomo]) -> bool {
    requeridos
        .iter()
        .all(|requerido| subconjunto.contains(requerido))
}

#[test]
fn core_es_suficiente_irreducible_y_canonico_para_la_asignacion_fallida() {
    let estructura = estructura_con_hoja();
    let formula = formula(
        "(forall ((x N)) :guarda (and (Type x a) (Type x a) (HasLab x \"synthetic-label\")) (not (and (Parent x @n/root) (Nivel x alto) (Parent x @n/root))))",
    );
    let instancia = instancia_falsa(&formula, &estructura);
    let mut presupuesto = PresupuestoExplicacion::nuevo(1_000_000);

    let ResultadoPresencia::Core(core) =
        extraer_core_presencia(&formula, &instancia, &estructura, &mut presupuesto).unwrap()
    else {
        panic!("el fragmento operacional debía aplicar");
    };

    let requeridos = [
        parsear_hecho("Type(n/leaf, a)"),
        parsear_hecho("HasLab(n/leaf, \"synthetic-label\")"),
        parsear_hecho("Parent(n/leaf, n/root)"),
        parsear_hecho("Nivel(n/leaf, alto)"),
    ];
    let hechos = core.atomos_sensibles();
    assert_eq!(hechos.len(), requeridos.len());
    assert!(requeridos.iter().all(|hecho| hechos.contains(hecho)));
    assert!(hechos.windows(2).all(|par| par[0] <= par[1]));

    assert!(condicion_de_falsedad_satisfecha(&requeridos, hechos));
    for indice in 0..hechos.len() {
        let mut reducido = hechos.to_vec();
        reducido.remove(indice);
        assert!(!condicion_de_falsedad_satisfecha(&requeridos, &reducido));
    }

    let debug = format!("{core:?}");
    let display = core.to_string();
    for dato in ["n/leaf", "n/root", "synthetic-label", "Nivel", "alto"] {
        assert!(!debug.contains(dato));
        assert!(!display.contains(dato));
    }
    assert!(!core.atomos_sensibles().is_empty());
}

#[test]
fn formas_fuera_del_fragmento_no_producen_core() {
    let estructura = estructura_con_hoja();
    let formula = formula("(forall ((x N)) :guarda (Type x a) (not (= x @n/root)))");
    let instancia = instancia_falsa(&formula, &estructura);
    let mut presupuesto = PresupuestoExplicacion::nuevo(1_000_000);

    assert!(matches!(
        extraer_core_presencia(&formula, &instancia, &estructura, &mut presupuesto).unwrap(),
        ResultadoPresencia::NoAplicable
    ));
}

#[test]
fn soporta_aristas_y_guarda_atomica_como_conjuncion_unitaria() {
    let estructura = estructura_con_hoja();
    let formula_arista = formula(
        "(forall ((x N) (e E)) :guarda (and (Type x a) (HasLab x \"synthetic-label\") (Edge e @n/root @n/leaf rel) (HasLab e \"edge-label\")) (not (and (Parent x @n/root) (Nivel x alto))))",
    );
    let instancia = instancia_falsa(&formula_arista, &estructura);
    let mut presupuesto = PresupuestoExplicacion::nuevo(1_000_000);

    let ResultadoPresencia::Core(core) =
        extraer_core_presencia(&formula_arista, &instancia, &estructura, &mut presupuesto).unwrap()
    else {
        panic!("Edge y HasLab(arista) pertenecen al fragmento");
    };
    let hechos = core.atomos_sensibles();
    let esperados = [
        parsear_hecho("Type(n/leaf, a)"),
        parsear_hecho("HasLab(n/leaf, \"synthetic-label\")"),
        parsear_hecho("Edge(n/root→n/leaf#rel#0, n/root, n/leaf, rel)"),
        parsear_hecho("HasLab(n/root→n/leaf#rel#0, \"edge-label\")"),
        parsear_hecho("Parent(n/leaf, n/root)"),
        parsear_hecho("Nivel(n/leaf, alto)"),
    ];
    assert_eq!(hechos.len(), esperados.len());
    assert!(esperados.iter().all(|hecho| hechos.contains(hecho)));

    let guarda_atomica =
        formula("(forall ((x N)) :guarda (Type x a) (not (and (Parent x @n/root))))");
    let instancia = instancia_falsa(&guarda_atomica, &estructura);
    let ResultadoPresencia::Core(core) =
        extraer_core_presencia(&guarda_atomica, &instancia, &estructura, &mut presupuesto).unwrap()
    else {
        panic!("una guarda atomica equivale a una conjuncion de un termino");
    };
    assert_eq!(core.atomos_sensibles().len(), 2);

    let sin_guarda = formula("(forall ((x N)) (not (and (Parent x @n/root))))");
    let instancia = instancia_falsa(&sin_guarda, &estructura);
    let ResultadoPresencia::Core(core) =
        extraer_core_presencia(&sin_guarda, &instancia, &estructura, &mut presupuesto).unwrap()
    else {
        panic!("una guarda ausente debe tratarse como verdadera");
    };
    assert_eq!(core.atomos_sensibles().len(), 1);
}

#[test]
fn presupuesto_agotado_descarta_el_core_sin_parcial() {
    let estructura = estructura_con_hoja();
    let formula =
        formula("(forall ((x N)) :guarda (and (Type x a)) (not (and (Parent x @n/root))))");
    let instancia = instancia_falsa(&formula, &estructura);
    for limite in [1, 2, 3] {
        let mut presupuesto = PresupuestoExplicacion::nuevo(limite);
        assert!(matches!(
            extraer_core_presencia(&formula, &instancia, &estructura, &mut presupuesto),
            Err(ErrorPresencia::PresupuestoAgotado { limite: agotado }) if agotado == limite
        ));
    }
}

fn estructura_para_core() -> Estructura {
    let mut estructura = Estructura::nueva(base()).unwrap();
    let raiz = estructura
        .insertar_nodo(
            ruta("n/PRIVATE_ROOT_CANARY"),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("PRIVATE_ROOT_LABEL"),
        )
        .unwrap();
    let hoja = estructura
        .insertar_nodo(
            ruta("n/PRIVATE_LEAF_CANARY"),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("PRIVATE_LABEL_CANARY"),
        )
        .unwrap();
    estructura.asignar_padre(hoja, raiz).unwrap();
    estructura
}

fn expansion_directa(formula: Formula, source_index: usize) -> ResultadoExpansionFuente {
    ResultadoExpansionFuente {
        oraciones: vec![OracionExpandida {
            formula,
            source_index,
            linea: Some(1),
            columna: Some(1),
            instance_index: None,
            sustituciones: Vec::new(),
        }],
        metricas: MetricasExpansion::default(),
    }
}

fn hay_violacion_presencia(hechos: &[Atomo], estructura: &Estructura) -> bool {
    let path_raiz = ruta("n/PRIVATE_ROOT_CANARY");
    let tipo_guarda = Sorte::nuevo("a");
    let etiqueta_guarda = Etiqueta::nuevo("PRIVATE_LABEL_CANARY");

    estructura.nodos().any(|(_, nodo)| {
        let guarda_tipo = hechos.iter().any(|hecho| {
            matches!(
                hecho,
                Atomo::Type { sujeto, sorte }
                    if sujeto == &nodo.path && sorte == &tipo_guarda
            )
        });
        let guarda_etiqueta = hechos.iter().any(|hecho| {
            matches!(
                hecho,
                Atomo::HasLab {
                    sujeto: SujetoAtomo::Nodo(sujeto),
                    etiqueta,
                } if sujeto == &nodo.path && etiqueta == &etiqueta_guarda
            )
        });
        let patron = hechos.iter().any(|hecho| {
            matches!(
                hecho,
                Atomo::Parent { hijo, padre }
                    if hijo == &nodo.path && padre == &path_raiz
            )
        });

        guarda_tipo && guarda_etiqueta && patron
    })
}

#[test]
fn explicar_extrae_core_suficiente_minimal_y_redactado_y_omite_formas_no_aplicables() {
    let estructura = estructura_para_core();
    let aplicable = formula(
        "(forall ((x N)) :guarda (and (Type x a) (HasLab x \"PRIVATE_LABEL_CANARY\")) (not (and (Parent x @n/PRIVATE_ROOT_CANARY))))",
    );
    let entrada = expansion_directa(aplicable, 4);
    let resultado = explicar(&entrada, &estructura, ModoExplicacion::Decidir).unwrap();
    let ResultadoExplicaciones::Decision(Some(objeto)) = resultado else {
        panic!("la fórmula universal aplicable es falsa en la hoja")
    };
    let core = objeto
        .core_presencia()
        .expect("el getter explícito entrega el core aplicable");

    // El getter es deliberadamente sensible; se usa aquí solo para replay.
    let hechos = core.atomos_sensibles();
    assert_eq!(hechos.len(), 3);
    assert!(hay_violacion_presencia(hechos, &estructura));
    for indice in 0..hechos.len() {
        let mut reducido = hechos.to_vec();
        reducido.remove(indice);
        assert!(
            !hay_violacion_presencia(&reducido, &estructura),
            "quitar el hecho {indice} debe destruir la violación de esta asignación"
        );
    }

    let debug_core = format!("{core:?}");
    let display_core = core.to_string();
    let debug_objeto = format!("{objeto:?}");
    for canario in [
        "PRIVATE_ROOT_CANARY",
        "PRIVATE_LEAF_CANARY",
        "PRIVATE_ROOT_LABEL",
        "PRIVATE_LABEL_CANARY",
    ] {
        assert!(!debug_core.contains(canario));
        assert!(!display_core.contains(canario));
        assert!(!debug_objeto.contains(canario));
    }

    let no_aplicable =
        formula("(forall ((x N)) :guarda (Type x a) (not (= x @n/PRIVATE_LEAF_CANARY)))");
    let entrada_no_aplicable = expansion_directa(no_aplicable, 5);
    let resultado = explicar(&entrada_no_aplicable, &estructura, ModoExplicacion::Decidir).unwrap();
    let ResultadoExplicaciones::Decision(Some(objeto)) = resultado else {
        panic!("la fórmula universal no aplicable también es falsa")
    };
    assert!(objeto.core_presencia().is_none());
}
