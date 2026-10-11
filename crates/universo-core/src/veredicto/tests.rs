use super::*;

use crate::estructura::Estructura;
use crate::evaluador::evaluar_trazable_con_presupuestos_medidos;
use crate::evaluador::{EvidenciaEvaluacion, ReglaEvidencia, TipoCuantificador, ValorLigadura};
use crate::formula::{Formula, Sentencia, SorteVar, TerminoNodo, TerminoSorte, Var};
use crate::invariante::expansion::{MetricasExpansion, OracionExpandida, ResultadoExpansionFuente};
use crate::manifiesto::Manifiesto;
use crate::path::BundlePath;
use crate::sexpr::parsear;
use crate::tipos::{Etiqueta, Sorte};
use crate::{DeclArista, DeclPredicado, Layout};
use std::collections::BTreeMap;

fn manifiesto_prueba() -> Manifiesto {
    let mut sortes_nodo_disco = BTreeMap::new();
    sortes_nodo_disco.insert(Sorte::nuevo("Patrón"), "pattern".to_string());
    Manifiesto {
        version: "veredicto-test/1".into(),
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

fn estructura_basica(invertir_insercion: bool) -> Estructura {
    let mut estructura = Estructura::nueva(manifiesto_prueba()).unwrap();
    let nodos = [("n/a", "a", "label-a"), ("n/b", "b", "label-b")];
    let indices: Vec<usize> = if invertir_insercion {
        vec![1, 0]
    } else {
        vec![0, 1]
    };
    for indice in indices {
        let (path, tipo, etiqueta) = nodos[indice];
        estructura
            .insertar_nodo(
                BundlePath::nuevo(path).unwrap(),
                Sorte::nuevo(tipo),
                Etiqueta::nuevo(etiqueta),
            )
            .unwrap();
    }
    estructura
}

fn formula(texto: &str) -> Formula {
    match parsear(texto).unwrap() {
        Sentencia::Oracion(formula) => formula,
        Sentencia::Esquema { .. } => panic!("se esperaba una oración"),
    }
}

fn expansion(
    especificaciones: impl IntoIterator<Item = (Formula, usize, Option<u64>)>,
) -> ResultadoExpansionFuente {
    let oraciones: Vec<_> = especificaciones
        .into_iter()
        .enumerate()
        .map(
            |(indice, (formula, source_index, instance_index))| OracionExpandida {
                formula,
                source_index,
                linea: Some(indice + 10),
                columna: Some(indice + 3),
                instance_index,
                sustituciones: Vec::new(),
            },
        )
        .collect();
    ResultadoExpansionFuente {
        metricas: MetricasExpansion {
            plantillas: 1,
            instancias: oraciones
                .iter()
                .filter(|oracion| oracion.instance_index.is_some())
                .count() as u64,
        },
        oraciones,
    }
}

fn formula_falsa() -> Formula {
    formula("(= @n/a @n/b)")
}

#[test]
fn decision_admitida_no_produce_objetos_de_explicacion() {
    let estructura = estructura_basica(false);
    let expansion = expansion([(formula("(Type @n/a a)"), 0, None)]);

    let resultado = explicar(&expansion, &estructura, ModoExplicacion::Decidir).unwrap();

    assert!(matches!(resultado, ResultadoExplicaciones::Decision(None)));
}

#[test]
fn decidir_selecciona_la_primera_instancia_fallida_en_orden_de_entrada() {
    let estructura = estructura_basica(false);
    let expansion = expansion([
        (formula("(Type @n/a a)"), 0, None),
        (formula_falsa(), 4, Some(7)),
        (formula_falsa(), 9, Some(2)),
    ]);

    let resultado = explicar(&expansion, &estructura, ModoExplicacion::Decidir).unwrap();
    let ResultadoExplicaciones::Decision(Some(objeto)) = resultado else {
        panic!("la primera fórmula falsa debe producir una decisión explicada")
    };

    assert_eq!(objeto.procedencia.source_index, 4);
    assert_eq!(objeto.procedencia.instance_index, Some(7));
    assert_eq!(objeto.procedencia.linea, Some(11));
    assert_eq!(objeto.procedencia.columna, Some(4));
}

#[test]
fn enumerar_emite_un_objeto_por_instancia_fallida_con_procedencia() {
    let estructura = estructura_basica(false);
    let entrada = expansion([
        (formula_falsa(), 6, Some(0)),
        (formula_falsa(), 6, Some(1)),
        (formula_falsa(), 6, Some(2)),
    ]);

    let resultado = explicar(
        &entrada,
        &estructura,
        ModoExplicacion::Enumerar {
            maximo_resultados: 10,
        },
    )
    .unwrap();
    let ResultadoExplicaciones::Enumeracion {
        objetos,
        truncated,
        reason,
    } = resultado
    else {
        panic!("se esperaba enumeración")
    };

    assert_eq!(objetos.len(), 3);
    assert!(!truncated);
    assert_eq!(reason, None);
    assert_eq!(
        objetos
            .iter()
            .map(|objeto| objeto.procedencia.instance_index)
            .collect::<Vec<_>>(),
        vec![Some(0), Some(1), Some(2)]
    );
    assert!(
        objetos
            .iter()
            .all(|objeto| objeto.procedencia.source_index == 6)
    );
}

fn enumerar_fallas(cantidad: usize) -> ResultadoExplicaciones {
    let estructura = estructura_basica(false);
    let entrada = expansion((0..cantidad).map(|indice| (formula_falsa(), 3, Some(indice as u64))));
    explicar(
        &entrada,
        &estructura,
        ModoExplicacion::Enumerar {
            maximo_resultados: 100,
        },
    )
    .unwrap()
}

#[test]
fn truncamiento_se_activa_solo_despues_de_detectar_la_falla_101() {
    assert_eq!(
        ModoExplicacion::enumerar_por_defecto(),
        ModoExplicacion::Enumerar {
            maximo_resultados: 100,
        }
    );

    let cien = enumerar_fallas(100);
    let ResultadoExplicaciones::Enumeracion {
        objetos: cien_objetos,
        truncated: cien_truncated,
        reason: cien_reason,
    } = cien
    else {
        panic!("se esperaba enumeración")
    };
    assert_eq!(cien_objetos.len(), 100);
    assert!(!cien_truncated);
    assert_eq!(cien_reason, None);

    let ciento_uno = enumerar_fallas(101);
    let ResultadoExplicaciones::Enumeracion {
        objetos: prefijo,
        truncated,
        reason,
    } = ciento_uno
    else {
        panic!("se esperaba enumeración")
    };
    assert_eq!(prefijo.len(), 100);
    assert!(truncated);
    assert_eq!(reason, Some(RazonTruncamiento::ResultLimit));
    assert_eq!(
        prefijo
            .iter()
            .map(|objeto| objeto.procedencia.instance_index)
            .collect::<Vec<_>>(),
        (0..100).map(Some).collect::<Vec<_>>()
    );
}

#[test]
fn agotamiento_de_presupuesto_devuelve_error_sin_resultado_parcial() {
    let estructura = estructura_basica(false);
    let entrada = expansion([(formula_falsa(), 2, Some(0)), (formula_falsa(), 2, Some(1))]);

    let error = explicar_con_presupuesto(
        &entrada,
        &estructura,
        ModoExplicacion::Enumerar {
            maximo_resultados: 100,
        },
        0,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ErrorVeredicto::PresupuestoAgotado { limite: 0 }
    ));
    let debug = format!("{error:?}");
    let display = error.to_string();
    assert!(!debug.contains("n/a"));
    assert!(!display.contains("n/a"));
}

fn programa_mixto_de_presupuesto() -> ResultadoExpansionFuente {
    expansion([
        (formula("(Type @n/a a)"), 0, None),
        (formula("(Type @n/b b)"), 1, None),
        (formula_falsa(), 2, None),
        (formula("(Type @n/a a)"), 3, None),
        (formula_falsa(), 4, None),
    ])
}

fn medir_explicacion(
    programa: &ResultadoExpansionFuente,
    estructura: &Estructura,
    modo: ModoExplicacion,
) -> ResultadoExplicacionesMedido {
    explicar_con_presupuesto_y_consumos(programa, estructura, modo, 1_000_000).unwrap()
}

#[test]
fn presupuesto_global_y_metricas_son_acumulados_y_deterministas_en_ambos_modos() {
    let estructura = estructura_basica(false);
    let programa = programa_mixto_de_presupuesto();
    let medicion_directa_a = evaluar_trazable_con_presupuestos_medidos(
        &programa.oraciones[2].formula,
        &estructura,
        1_000_000,
        1_000_000,
    )
    .unwrap();
    let medicion_directa_b = evaluar_trazable_con_presupuestos_medidos(
        &programa.oraciones[2].formula,
        &estructura,
        1_000_000,
        1_000_000,
    )
    .unwrap();
    assert!(!medicion_directa_a.resultado.valor);
    assert_eq!(
        medicion_directa_a.unidades_semanticas,
        medicion_directa_b.unidades_semanticas
    );
    assert_eq!(
        medicion_directa_a.unidades_evidencia,
        medicion_directa_b.unidades_evidencia
    );
    assert!(medicion_directa_a.unidades_semanticas > 0);
    assert!(medicion_directa_a.unidades_evidencia > 0);

    let modo_decidir = ModoExplicacion::Decidir;
    let modo_enumerar = ModoExplicacion::Enumerar {
        maximo_resultados: 10,
    };
    let decidir = medir_explicacion(&programa, &estructura, modo_decidir);
    let decidir_repetido = medir_explicacion(&programa, &estructura, modo_decidir);
    let enumerar = medir_explicacion(&programa, &estructura, modo_enumerar);
    let enumerar_repetido = medir_explicacion(&programa, &estructura, modo_enumerar);

    assert_eq!(
        explicar(&programa, &estructura, modo_decidir).unwrap(),
        decidir.resultado
    );
    assert_eq!(
        explicar(&programa, &estructura, modo_enumerar).unwrap(),
        enumerar.resultado
    );

    for (primera, repetida) in [
        (&decidir, &decidir_repetido),
        (&enumerar, &enumerar_repetido),
    ] {
        assert_eq!(primera.unidades_semanticas, repetida.unidades_semanticas);
        assert_eq!(primera.unidades_evidencia, repetida.unidades_evidencia);
        assert_eq!(primera.unidades_agregador, repetida.unidades_agregador);
        assert_eq!(primera.unidades_consumidas, repetida.unidades_consumidas);
        assert_eq!(
            primera.unidades_consumidas,
            primera.unidades_semanticas + primera.unidades_evidencia + primera.unidades_agregador
        );
    }
    assert!(enumerar.unidades_consumidas > decidir.unidades_consumidas);
    let ResultadoExplicaciones::Decision(Some(_)) = decidir.resultado else {
        panic!("Decidir debe devolver la primera instancia falsa")
    };
    let ResultadoExplicaciones::Enumeracion {
        objetos,
        truncated: false,
        reason: None,
    } = enumerar.resultado
    else {
        panic!("Enumerar debe devolver ambas fallas sin truncamiento")
    };
    assert_eq!(objetos.len(), 2);
    assert_eq!(
        objetos
            .iter()
            .map(|objeto| objeto.procedencia.source_index)
            .collect::<Vec<_>>(),
        vec![2, 4]
    );

    // Para cada modo, medir el total y probar el límite exacto y el anterior.
    // Si el saldo se reiniciara por fórmula, el límite global−1 no agotaría el
    // consumo acumulado de estas varias fórmulas verdaderas y falsas.
    for (modo, exacto) in [
        (modo_decidir, decidir.unidades_consumidas),
        (modo_enumerar, enumerar.unidades_consumidas),
    ] {
        assert!(exacto > 0);
        let error = explicar_con_presupuesto_y_consumos(&programa, &estructura, modo, exacto - 1)
            .unwrap_err();
        assert!(matches!(
            error,
            ErrorVeredicto::PresupuestoAgotado { limite } if limite == exacto - 1
        ));

        let justo =
            explicar_con_presupuesto_y_consumos(&programa, &estructura, modo, exacto).unwrap();
        assert_eq!(justo.unidades_consumidas, exacto);
        assert_eq!(
            explicar_con_presupuesto(&programa, &estructura, modo, exacto).unwrap(),
            justo.resultado
        );
    }
}

fn resultado_de_insercion(invertir: bool) -> (Estructura, ResultadoExplicaciones) {
    let estructura = estructura_basica(invertir);
    let entrada = expansion([(
        formula("(forall ((testigo N)) (Type testigo a))"),
        5,
        Some(11),
    )]);
    let resultado = explicar(&entrada, &estructura, ModoExplicacion::Decidir).unwrap();
    (estructura, resultado)
}

#[test]
fn insercion_distinta_de_los_mismos_objetos_no_cambia_el_resultado() {
    let (_, normal) = resultado_de_insercion(false);
    let (_, invertido) = resultado_de_insercion(true);

    assert_eq!(normal, invertido);
}

fn replay_oraculo_forall_type(
    formula: &Formula,
    evidencia: &EvidenciaEvaluacion,
    estructura: &Estructura,
) -> bool {
    let Formula::ParaTodo {
        ligaduras,
        guarda: None,
        cuerpo,
    } = formula
    else {
        panic!("el fixture del oráculo es un forall sin guarda")
    };
    let [(variable, SorteVar::Nodo)] = ligaduras.as_slice() else {
        panic!("el fixture liga una variable de nodo")
    };
    let Formula::Type {
        sujeto: TerminoNodo::Var(variable_cuerpo),
        sorte: TerminoSorte::Literal(sorte_esperada),
    } = cuerpo.as_ref()
    else {
        panic!("el cuerpo del fixture es Type(x, K)")
    };
    assert_eq!(variable, variable_cuerpo);

    let ReglaEvidencia::Cuantificador {
        tipo: TipoCuantificador::ParaTodo,
        instancias,
        ..
    } = &evidencia.regla
    else {
        panic!("la evidencia conserva el nodo forall")
    };
    let contraejemplo = instancias
        .iter()
        .find(|instancia| !instancia.valor)
        .expect("un forall falso tiene una instancia falsa");
    let Some(ligadura) = contraejemplo
        .asignacion
        .ligaduras
        .iter()
        .find(|ligadura| ligadura.variable == *variable)
    else {
        panic!("la instancia incluye la variable cuantificada")
    };
    let ValorLigadura::Nodo(path) = &ligadura.valor else {
        panic!("la variable del fixture es de nodo")
    };
    let id = estructura
        .por_path(path)
        .expect("la asignación de evidencia resuelve a un nodo del snapshot");
    let nodo = estructura.nodo(id).unwrap();

    // Oráculo independiente del evaluator: reinterpreta el único átomo del
    // cuerpo a partir del valor concreto ligado en la evidencia.
    nodo.tipo == *sorte_esperada
}

#[test]
fn contraejemplo_se_reproduce_con_el_ast_y_un_oraculo_independiente() {
    let estructura = estructura_basica(false);
    let fuente = formula("(forall ((testigo N)) (Type testigo a))");
    let entrada = expansion([(fuente.clone(), 2, Some(0))]);
    let resultado = explicar(&entrada, &estructura, ModoExplicacion::Decidir).unwrap();
    let ResultadoExplicaciones::Decision(Some(objeto)) = resultado else {
        panic!("el forall del fixture es falso")
    };
    let evidencia = &objeto.counterexample.evidencia;

    assert!(!evidencia.valor);
    assert!(!replay_oraculo_forall_type(&fuente, evidencia, &estructura));
}

fn estructura_con_canarios() -> Estructura {
    let mut manifiesto = manifiesto_prueba();
    manifiesto.predicados.push(DeclPredicado {
        nombre: "PRIVATE_PREDICATE".into(),
        clave: "private".into(),
        dominio: ["PRIVATE_CONSTANT_CANARY".to_string()]
            .into_iter()
            .collect(),
    });
    let mut estructura = Estructura::nueva(manifiesto).unwrap();
    estructura
        .insertar_nodo(
            BundlePath::nuevo("n/PRIVATE_PATH_CANARY").unwrap(),
            Sorte::nuevo("b"),
            Etiqueta::nuevo("PRIVATE_LABEL_CANARY"),
        )
        .unwrap();
    estructura
}

#[test]
fn debug_y_display_no_revelan_valores_del_bundle_ni_de_la_forma() {
    let estructura = estructura_con_canarios();
    let fuente = formula(
        "(forall ((PRIVATE_VARIABLE_CANARY N)) (PRIVATE_PREDICATE PRIVATE_VARIABLE_CANARY PRIVATE_CONSTANT_CANARY))",
    );
    let entrada = expansion([(fuente, 1, Some(0))]);
    let resultado = explicar(
        &entrada,
        &estructura,
        ModoExplicacion::Enumerar {
            maximo_resultados: 10,
        },
    )
    .unwrap();
    let depuracion = format!("{resultado:?}");
    for canario in [
        "PRIVATE_PATH_CANARY",
        "PRIVATE_LABEL_CANARY",
        "PRIVATE_PREDICATE",
        "PRIVATE_CONSTANT_CANARY",
        "PRIVATE_VARIABLE_CANARY",
    ] {
        assert!(
            !depuracion.contains(canario),
            "se filtró {canario}: {depuracion}"
        );
    }

    let formula_invalida = Formula::Type {
        sujeto: TerminoNodo::Var(Var::nuevo("PRIVATE_UNBOUND_CANARY")),
        sorte: TerminoSorte::Literal(Sorte::nuevo("a")),
    };
    let entrada_invalida = expansion([(formula_invalida, 9, None)]);
    let error = explicar(&entrada_invalida, &estructura, ModoExplicacion::Decidir).unwrap_err();
    let depuracion_error = format!("{error:?}");
    let display_error = error.to_string();
    assert!(!depuracion_error.contains("PRIVATE_UNBOUND_CANARY"));
    assert!(!display_error.contains("PRIVATE_UNBOUND_CANARY"));
}
