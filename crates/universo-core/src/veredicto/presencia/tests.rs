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

fn estructura_con_dos_hojas(reverse_insertion: bool, leaf_a_aplicable: bool) -> Estructura {
    let mut estructura = Estructura::nueva(base()).unwrap();
    let rows = [
        ("n/root", "root-title"),
        (
            "n/leaf-a",
            if leaf_a_aplicable {
                "synthetic-label"
            } else {
                "nonmatching-label"
            },
        ),
        ("n/leaf-b", "synthetic-label"),
    ];
    let order = if reverse_insertion {
        [2, 1, 0]
    } else {
        [0, 1, 2]
    };
    let mut ids = [None; 3];
    for index in order {
        let (path_text, label) = rows[index];
        ids[index] = Some(
            estructura
                .insertar_nodo(ruta(path_text), Sorte::nuevo("a"), Etiqueta::nuevo(label))
                .unwrap(),
        );
    }
    for id in ids.iter().skip(1).flatten().copied() {
        estructura.asignar_padre(id, ids[0].unwrap()).unwrap();
        estructura.asignar_atributo(id, "Nivel", "alto").unwrap();
    }
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

fn estructura_reducida_para_core(
    hechos: &[Atomo],
    path_raiz: &str,
    path_hoja: &str,
    etiqueta_hoja: &str,
) -> Estructura {
    let contiene = |texto: &str| hechos.contains(&parsear_hecho(texto));
    let tiene_tipo = contiene(&format!("Type({path_hoja}, a)"));
    let tiene_etiqueta = contiene(&format!("HasLab({path_hoja}, \"{etiqueta_hoja}\")"));
    let tiene_padre = contiene(&format!("Parent({path_hoja}, {path_raiz})"));
    let tiene_nivel = contiene(&format!("Nivel({path_hoja}, alto)"));

    let mut estructura = Estructura::nueva(base()).unwrap();
    let raiz = estructura
        .insertar_nodo(
            ruta(path_raiz),
            Sorte::nuevo("a"),
            // Mantiene el literal de HasLab en uso aun cuando se quitó del
            // nodo asignado. La raíz no falsifica la fórmula porque no tiene
            // el hecho Parent requerido por el cuerpo.
            Etiqueta::nuevo(etiqueta_hoja),
        )
        .unwrap();
    let hoja = estructura
        .insertar_nodo(
            ruta(path_hoja),
            Sorte::nuevo(if tiene_tipo { "a" } else { "Retirado" }),
            Etiqueta::nuevo(if tiene_etiqueta {
                etiqueta_hoja
            } else {
                "unmatched-title"
            }),
        )
        .unwrap();
    if tiene_padre {
        estructura.asignar_padre(hoja, raiz).unwrap();
    }
    if tiene_nivel {
        estructura.asignar_atributo(hoja, "Nivel", "alto").unwrap();
    }
    estructura
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

    let replay = estructura_reducida_para_core(hechos, "n/root", "n/leaf", "synthetic-label");
    assert!(!crate::evaluador::evaluar(&formula, &replay).unwrap());
    for indice in 0..hechos.len() {
        let mut reducido = hechos.to_vec();
        reducido.remove(indice);
        let replay_reducido =
            estructura_reducida_para_core(&reducido, "n/root", "n/leaf", "synthetic-label");
        assert!(crate::evaluador::evaluar(&formula, &replay_reducido).unwrap());
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

#[test]
fn explicar_extrae_core_suficiente_minimal_y_redactado_y_omite_formas_no_aplicables() {
    let estructura = estructura_para_core();
    let aplicable = formula(
        "(forall ((x N)) :guarda (and (Type x a) (HasLab x \"PRIVATE_LABEL_CANARY\")) (not (and (Parent x @n/PRIVATE_ROOT_CANARY))))",
    );
    let entrada = expansion_directa(aplicable.clone(), 4);
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
    let replay = estructura_reducida_para_core(
        hechos,
        "n/PRIVATE_ROOT_CANARY",
        "n/PRIVATE_LEAF_CANARY",
        "PRIVATE_LABEL_CANARY",
    );
    assert!(!crate::evaluador::evaluar(&aplicable, &replay).unwrap());
    for indice in 0..hechos.len() {
        let mut reducido = hechos.to_vec();
        reducido.remove(indice);
        let replay_reducido = estructura_reducida_para_core(
            &reducido,
            "n/PRIVATE_ROOT_CANARY",
            "n/PRIVATE_LEAF_CANARY",
            "PRIVATE_LABEL_CANARY",
        );
        assert!(
            crate::evaluador::evaluar(&aplicable, &replay_reducido).unwrap(),
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

fn extraer_core_de_instancia(
    formula: &Formula,
    instancia: &InstanciaCuantificada,
    estructura: &Estructura,
) -> CorePresencia {
    let mut presupuesto = PresupuestoExplicacion::nuevo(1_000_000);
    let ResultadoPresencia::Core(core) =
        extraer_core_presencia(formula, instancia, estructura, &mut presupuesto).unwrap()
    else {
        panic!("cada asignación falsificante del fixture debe tener un core");
    };
    core
}

fn assert_core_minimal_para_leaf(formula: &Formula, core: &CorePresencia, path_leaf: &str) {
    let requeridos = [
        parsear_hecho(&format!("Type({path_leaf}, a)")),
        parsear_hecho(&format!("HasLab({path_leaf}, \"synthetic-label\")")),
        parsear_hecho(&format!("Parent({path_leaf}, n/root)")),
        parsear_hecho(&format!("Nivel({path_leaf}, alto)")),
    ];
    let hechos = core.atomos_sensibles();
    assert_eq!(hechos.len(), requeridos.len());
    assert!(requeridos.iter().all(|hecho| hechos.contains(hecho)));
    let replay = estructura_reducida_para_core(hechos, "n/root", path_leaf, "synthetic-label");
    assert!(
        !crate::evaluador::evaluar(formula, &replay).unwrap(),
        "el snapshot reducido al core debe conservar el contraejemplo para {path_leaf}"
    );
    for indice in 0..hechos.len() {
        let mut reducido = hechos.to_vec();
        reducido.remove(indice);
        let replay_reducido =
            estructura_reducida_para_core(&reducido, "n/root", path_leaf, "synthetic-label");
        assert!(
            crate::evaluador::evaluar(formula, &replay_reducido).unwrap(),
            "cada átomo del core para {path_leaf} debe ser necesario"
        );
    }
}

#[test]
fn dos_asignaciones_incomparables_eligen_un_core_canonico_y_minimal() {
    let formula = formula(
        "(forall ((x N)) :guarda (and (Type x a) (HasLab x \"synthetic-label\")) (not (and (Parent x @n/root) (Nivel x alto))))",
    );
    let mut resultados = Vec::new();

    for reverse_insertion in [false, true] {
        let estructura_ambas_aplicables = estructura_con_dos_hojas(reverse_insertion, true);
        let traza_primera = evaluar_trazable(&formula, &estructura_ambas_aplicables).unwrap();
        assert!(!traza_primera.valor);
        let primera = instancia_falsa(&formula, &estructura_ambas_aplicables);
        let crate::evaluador::ValorLigadura::Nodo(path_primero) =
            &primera.asignacion.ligaduras[0].valor
        else {
            panic!("la primera asignación debe ligar un nodo");
        };
        assert_eq!(path_primero.como_str(), "n/leaf-a");
        let core_primero =
            extraer_core_de_instancia(&formula, &primera, &estructura_ambas_aplicables);

        // Una segunda evaluación real vuelve vacua la guarda de leaf-a, así
        // que leaf-b es el primer binding falsificante de esa traza.
        let estructura_solo_leaf_b_aplicable = estructura_con_dos_hojas(reverse_insertion, false);
        let traza_segunda = evaluar_trazable(&formula, &estructura_solo_leaf_b_aplicable).unwrap();
        assert!(!traza_segunda.valor);
        let ReglaEvidencia::Cuantificador {
            tipo: TipoCuantificador::ParaTodo,
            instancias,
            ..
        } = &traza_segunda.evidencia.regla
        else {
            panic!("la traza real debe ser un cuantificador universal");
        };
        let leaf_a = instancias
            .iter()
            .find(|instancia| {
                matches!(
                    &instancia.asignacion.ligaduras[0].valor,
                    crate::evaluador::ValorLigadura::Nodo(path)
                        if path.como_str() == "n/leaf-a"
                )
            })
            .expect("leaf-a debe aparecer en la traza vacua");
        assert!(!leaf_a.guarda.as_deref().unwrap().valor);
        assert!(leaf_a.valor, "una guarda falsa es vacua");

        let segunda = instancia_falsa(&formula, &estructura_solo_leaf_b_aplicable);
        let crate::evaluador::ValorLigadura::Nodo(path_segundo) =
            &segunda.asignacion.ligaduras[0].valor
        else {
            panic!("la segunda asignación debe ligar un nodo");
        };
        assert_eq!(path_segundo.como_str(), "n/leaf-b");
        let core_segundo =
            extraer_core_de_instancia(&formula, &segunda, &estructura_solo_leaf_b_aplicable);

        assert_core_minimal_para_leaf(&formula, &core_primero, "n/leaf-a");
        assert_core_minimal_para_leaf(&formula, &core_segundo, "n/leaf-b");
        assert!(
            core_primero
                .atomos_sensibles()
                .iter()
                .all(|hecho| !core_segundo.atomos_sensibles().contains(hecho)),
            "los dos cores deben ser incomparables"
        );
        assert!(
            core_segundo
                .atomos_sensibles()
                .iter()
                .all(|hecho| !core_primero.atomos_sensibles().contains(hecho)),
            "los dos cores deben ser incomparables"
        );

        let expansion = expansion_directa(formula.clone(), 0);
        let resultado = explicar(
            &expansion,
            &estructura_ambas_aplicables,
            ModoExplicacion::Enumerar {
                maximo_resultados: 100,
            },
        )
        .unwrap();
        let ResultadoExplicaciones::Enumeracion {
            objetos,
            truncated: false,
            reason: None,
        } = resultado
        else {
            panic!("se esperaba la enumeración completa de una fórmula");
        };
        assert_eq!(objetos.len(), 1, "los otros cores no multiplican objetos");
        let objeto = &objetos[0];
        let core_elegido = objeto.core_presencia().expect("debe incluir un core");
        assert_eq!(core_elegido, &core_primero);
        assert!(objeto.gap().is_none());
        resultados.push((core_elegido.clone(), core_segundo));
    }

    assert_eq!(resultados[0], resultados[1]);
}
