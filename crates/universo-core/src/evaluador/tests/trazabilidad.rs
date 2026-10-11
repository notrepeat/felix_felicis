use super::*;

use crate::IdArista;
use crate::evaluador::{
    AsignacionCanonica, ErrorEvaluacion, InstanciaCuantificada, LigaduraCanonica,
    PRESUPUESTO_EVIDENCIA_POR_DEFECTO, ReglaEvidencia, ResultadoTrazable, TipoCuantificador,
    ValorLigadura, evaluar_trazable, evaluar_trazable_con_presupuesto,
    evaluar_trazable_con_presupuesto_total_medido, evaluar_trazable_con_presupuestos,
    evaluar_trazable_con_presupuestos_medidos,
};
use crate::formula::{Formula, SorteVar, TerminoArista, TerminoNodo, Var};
use crate::ids::AristaId;
use crate::path::BundlePath;

fn ruta(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).unwrap()
}

fn identidad_arista(estructura: &Estructura, id: AristaId) -> Option<IdArista> {
    let arista = estructura.arista(id)?;
    Some(IdArista {
        origen: estructura.nodo(arista.origen)?.path.clone(),
        sorte: arista.tipo.clone(),
        destino: estructura.nodo(arista.destino)?.path.clone(),
        ordinal: arista.ordinal,
    })
}

fn arista_con_identidad(estructura: &Estructura, identidad: &IdArista) -> Option<AristaId> {
    estructura
        .aristas()
        .map(|(id, _)| id)
        .find(|id| identidad_arista(estructura, *id).as_ref() == Some(identidad))
}

fn oraculo_con_asignacion(
    formula: &Formula,
    estructura: &Estructura,
    asignacion: &AsignacionCanonica,
) -> bool {
    let mut entorno = AsignacionOraculo::default();
    for ligadura in &asignacion.ligaduras {
        match &ligadura.valor {
            ValorLigadura::Nodo(path) => entorno.ligar_nodo(
                ligadura.variable.clone(),
                estructura
                    .por_path(path)
                    .expect("la ruta canónica existe en el fixture"),
            ),
            ValorLigadura::Arista(identidad) => entorno.ligar_arista(
                ligadura.variable.clone(),
                arista_con_identidad(estructura, identidad)
                    .expect("la identidad canónica existe en el fixture"),
            ),
        }
    }
    oraculo(formula, estructura, &mut entorno).unwrap()
}

fn comprobar_instancias_con_oraculo(
    guardia: Option<&Formula>,
    cuerpo: &Formula,
    instancias: &[InstanciaCuantificada],
    universal: bool,
    estructura: &Estructura,
) {
    for instancia in instancias {
        let cubre = if let Some(guardia) = guardia {
            let resultado = oraculo_con_asignacion(guardia, estructura, &instancia.asignacion);
            let evidencia = instancia
                .guarda
                .as_deref()
                .expect("la evidencia registra una guarda presente");
            assert_eq!(evidencia.valor, resultado);
            resultado
        } else {
            assert!(instancia.guarda.is_none());
            true
        };
        let esperado = if !cubre {
            universal
        } else {
            let resultado = oraculo_con_asignacion(cuerpo, estructura, &instancia.asignacion);
            let evidencia = instancia
                .cuerpo
                .as_deref()
                .expect("la evidencia registra el cuerpo cuando aplica");
            assert_eq!(evidencia.valor, resultado);
            resultado
        };
        if !cubre {
            assert!(instancia.cuerpo.is_none());
        }
        assert_eq!(instancia.valor, esperado);
    }
}

fn comprobar_resultado(formula: &Formula, estructura: &Estructura) -> ResultadoTrazable {
    let resultado = evaluar_trazable(formula, estructura).unwrap();
    let valor_oraculo = oraculo(formula, estructura, &mut AsignacionOraculo::default()).unwrap();
    assert_eq!(resultado.valor, evaluar(formula, estructura).unwrap());
    assert_eq!(resultado.valor, valor_oraculo);
    assert_eq!(resultado.evidencia.valor, resultado.valor);
    resultado
}

fn constructor_atomico(formula: &Formula) -> ConstructorAtomico {
    match formula {
        Formula::Type { .. } => ConstructorAtomico::Type,
        Formula::Edge { .. } => ConstructorAtomico::Edge,
        Formula::Parent { .. } => ConstructorAtomico::Parent,
        Formula::HasLab {
            sujeto: crate::formula::SujetoTermino::Nodo(_),
            ..
        } => ConstructorAtomico::HasLabNodo,
        Formula::HasLab {
            sujeto: crate::formula::SujetoTermino::Arista(_),
            ..
        } => ConstructorAtomico::HasLabArista,
        Formula::Ext { .. } => ConstructorAtomico::Ext,
        Formula::TcParent { .. } => ConstructorAtomico::TcParent,
        Formula::IgualNodo(..) => ConstructorAtomico::IgualNodo,
        Formula::IgualArista(..) => ConstructorAtomico::IgualArista,
        _ => panic!("no es una fórmula atómica"),
    }
}

fn dominios_canonicos(estructura: &Estructura, sorte: SorteVar) -> Vec<ValorLigadura> {
    match sorte {
        SorteVar::Nodo => {
            let mut rutas: Vec<_> = estructura
                .nodos()
                .map(|(_, nodo)| nodo.path.clone())
                .collect();
            rutas.sort();
            rutas.into_iter().map(ValorLigadura::Nodo).collect()
        }
        SorteVar::Arista => {
            let mut identidades: Vec<_> = estructura
                .aristas()
                .filter_map(|(id, _)| identidad_arista(estructura, id))
                .collect();
            identidades.sort_by_key(ToString::to_string);
            identidades.into_iter().map(ValorLigadura::Arista).collect()
        }
    }
}

fn combinaciones_canonicas(
    ligaduras: &[(Var, SorteVar)],
    estructura: &Estructura,
) -> Vec<AsignacionCanonica> {
    fn expandir(
        indice: usize,
        ligaduras: &[(Var, SorteVar)],
        dominios: &[Vec<ValorLigadura>],
        actuales: &mut Vec<LigaduraCanonica>,
        resultado: &mut Vec<AsignacionCanonica>,
    ) {
        if indice == ligaduras.len() {
            resultado.push(AsignacionCanonica {
                ligaduras: actuales.clone(),
            });
            return;
        }
        for valor in &dominios[indice] {
            actuales.push(LigaduraCanonica {
                variable: ligaduras[indice].0.clone(),
                valor: valor.clone(),
            });
            expandir(indice + 1, ligaduras, dominios, actuales, resultado);
            actuales.pop();
        }
    }

    let dominios: Vec<_> = ligaduras
        .iter()
        .map(|(_, sorte)| dominios_canonicos(estructura, *sorte))
        .collect();
    let mut resultado = Vec::new();
    if ligaduras.is_empty() {
        resultado.push(AsignacionCanonica::default());
    } else {
        expandir(0, ligaduras, &dominios, &mut Vec::new(), &mut resultado);
    }
    resultado
}

fn aplicar_asignacion_oraculo(
    entorno: &mut AsignacionOraculo,
    estructura: &Estructura,
    asignacion: &AsignacionCanonica,
) {
    for ligadura in &asignacion.ligaduras {
        match &ligadura.valor {
            ValorLigadura::Nodo(path) => entorno.ligar_nodo(
                ligadura.variable.clone(),
                estructura
                    .por_path(path)
                    .expect("el path de la asignación canónica debe existir"),
            ),
            ValorLigadura::Arista(id) => entorno.ligar_arista(
                ligadura.variable.clone(),
                arista_con_identidad(estructura, id)
                    .expect("la identidad de arista canónica debe existir"),
            ),
        }
    }
}

/// Verificador deliberadamente test-only: reproduce cada subárbol con el
/// oráculo independiente y comprueba constructor, alcance y orden de tuplas.
fn verificar_arbol_con_oraculo(
    formula: &Formula,
    evidencia: &crate::evaluador::EvidenciaEvaluacion,
    estructura: &Estructura,
    entorno: &AsignacionOraculo,
) -> bool {
    let mut asignacion_replay = entorno.clone();
    let esperado = oraculo(formula, estructura, &mut asignacion_replay).unwrap();
    assert_eq!(
        evidencia.valor, esperado,
        "valor de subárbol contra oráculo"
    );

    match formula {
        Formula::Type { .. }
        | Formula::Edge { .. }
        | Formula::Parent { .. }
        | Formula::HasLab { .. }
        | Formula::Ext { .. }
        | Formula::TcParent { .. }
        | Formula::IgualNodo(..)
        | Formula::IgualArista(..) => {
            let ReglaEvidencia::Atomo {
                constructor,
                camino_tc,
            } = &evidencia.regla
            else {
                panic!("la evidencia de átomo no coincide con el AST");
            };
            assert_eq!(*constructor, constructor_atomico(formula));
            assert_eq!(
                camino_tc.is_some(),
                matches!(formula, Formula::TcParent { .. }) && esperado
            );
            if let Some(camino) = camino_tc {
                assert!(!camino.is_empty());
                for tramo in camino.windows(2) {
                    let hijo = estructura.por_path(&tramo[0]).unwrap();
                    let padre = estructura.por_path(&tramo[1]).unwrap();
                    assert_eq!(estructura.padre(hijo), Some(padre));
                }
            }
        }
        Formula::No(interior) => {
            let ReglaEvidencia::No { interior: sub } = &evidencia.regla else {
                panic!("la evidencia de negación no coincide con el AST");
            };
            assert_eq!(
                !verificar_arbol_con_oraculo(interior, sub, estructura, entorno),
                esperado
            );
        }
        Formula::Y(partes) => {
            let ReglaEvidencia::Conjuncion { partes_evaluadas } = &evidencia.regla else {
                panic!("la evidencia de conjunción no coincide con el AST");
            };
            let mut cantidad_esperada = 0;
            for parte in partes {
                cantidad_esperada += 1;
                if !verificar_arbol_con_oraculo(
                    parte,
                    partes_evaluadas
                        .get(cantidad_esperada - 1)
                        .unwrap_or_else(|| panic!("falta una subevidencia de conjunción visitada")),
                    estructura,
                    entorno,
                ) {
                    break;
                }
            }
            assert_eq!(partes_evaluadas.len(), cantidad_esperada);
            assert_eq!(esperado, partes_evaluadas.iter().all(|parte| parte.valor));
        }
        Formula::O(partes) => {
            let ReglaEvidencia::Disyuncion { partes_evaluadas } = &evidencia.regla else {
                panic!("la evidencia de disyunción no coincide con el AST");
            };
            let mut cantidad_esperada = 0;
            for parte in partes {
                cantidad_esperada += 1;
                if verificar_arbol_con_oraculo(
                    parte,
                    partes_evaluadas
                        .get(cantidad_esperada - 1)
                        .unwrap_or_else(|| panic!("falta una subevidencia de disyunción visitada")),
                    estructura,
                    entorno,
                ) {
                    break;
                }
            }
            assert_eq!(partes_evaluadas.len(), cantidad_esperada);
            assert_eq!(esperado, partes_evaluadas.iter().any(|parte| parte.valor));
        }
        Formula::Implica(antecedente, consecuente) => {
            let ReglaEvidencia::Implicacion {
                antecedente: sub_a,
                consecuente: sub_b,
            } = &evidencia.regla
            else {
                panic!("la evidencia de implicación no coincide con el AST");
            };
            let valor_a = verificar_arbol_con_oraculo(antecedente, sub_a, estructura, entorno);
            match (valor_a, sub_b) {
                (false, None) => assert!(esperado),
                (true, Some(sub_b)) => assert_eq!(
                    verificar_arbol_con_oraculo(consecuente, sub_b, estructura, entorno),
                    esperado
                ),
                _ => panic!("la evidencia de implicación no respeta cortocircuito"),
            }
        }
        Formula::ParaTodo {
            ligaduras,
            guarda,
            cuerpo,
        }
        | Formula::Existe {
            ligaduras,
            guarda,
            cuerpo,
        } => {
            let universal = matches!(formula, Formula::ParaTodo { .. });
            let ReglaEvidencia::Cuantificador {
                tipo,
                instancias,
                busqueda_agotada,
            } = &evidencia.regla
            else {
                panic!("la evidencia cuantificada no coincide con el AST");
            };
            assert_eq!(
                *tipo,
                if universal {
                    TipoCuantificador::ParaTodo
                } else {
                    TipoCuantificador::Existe
                }
            );

            let combinaciones = combinaciones_canonicas(ligaduras, estructura);
            let mut valor_acumulado = universal;
            let mut cantidad_visitada = 0;
            let mut corto_circuito = false;
            for asignacion in &combinaciones {
                let instancia = instancias
                    .get(cantidad_visitada)
                    .expect("cada tupla visitada deja una instancia");
                assert_eq!(instancia.asignacion, *asignacion);
                let mut entorno_instancia = entorno.clone();
                aplicar_asignacion_oraculo(&mut entorno_instancia, estructura, asignacion);

                let cubierta = if let Some(guarda_ast) = guarda {
                    let guarda_evidencia = instancia
                        .guarda
                        .as_deref()
                        .expect("la guarda declarada tiene evidencia");
                    verificar_arbol_con_oraculo(
                        guarda_ast,
                        guarda_evidencia,
                        estructura,
                        &entorno_instancia,
                    )
                } else {
                    assert!(instancia.guarda.is_none());
                    true
                };
                let valor_instancia = if !cubierta {
                    assert!(instancia.cuerpo.is_none());
                    universal
                } else {
                    let cuerpo_evidencia = instancia
                        .cuerpo
                        .as_deref()
                        .expect("el cuerpo de una instancia cubierta tiene evidencia");
                    verificar_arbol_con_oraculo(
                        cuerpo,
                        cuerpo_evidencia,
                        estructura,
                        &entorno_instancia,
                    )
                };
                assert_eq!(instancia.valor, valor_instancia);
                cantidad_visitada += 1;
                if universal {
                    valor_acumulado &= valor_instancia;
                } else {
                    valor_acumulado |= valor_instancia;
                }
                if (universal && !valor_instancia) || (!universal && valor_instancia) {
                    corto_circuito = true;
                    break;
                }
            }
            assert_eq!(instancias.len(), cantidad_visitada);
            assert_eq!(*busqueda_agotada, !corto_circuito);
            assert_eq!(valor_acumulado, esperado);
        }
    }
    esperado
}

#[test]
fn traza_conectivos_e_igualdades_reproduce_las_ramas_visitadas() {
    let estructura = mundo().estructura;
    let casos = [
        "(not (Type @n/root a))",
        "(and (Type @n/root a) (Parent @n/root @n/middle))",
        "(or (Type @n/root b) (Parent @n/middle @n/root) (Type @n/solo a))",
        "(or (Type @n/root b) (Parent @n/root @n/middle))",
        "(=> (Type @n/root a) (Type @n/root b))",
        "(=> (Type @n/root b) (Type @n/root b))",
        "(= @n/root @n/leaf)",
        "(= @n/root→n/middle#rel#0 @n/root→n/middle#rel#1)",
    ];
    for texto in casos {
        let formula = oracion(texto);
        let resultado = comprobar_resultado(&formula, &estructura);
        assert!(matches!(
            resultado.evidencia.regla,
            ReglaEvidencia::No { .. }
                | ReglaEvidencia::Conjuncion { .. }
                | ReglaEvidencia::Disyuncion { .. }
                | ReglaEvidencia::Implicacion { .. }
                | ReglaEvidencia::Atomo { .. }
        ));
    }

    let implicacion = oracion("(=> (Type @n/root b) (Type @n/root a))");
    let resultado = comprobar_resultado(&implicacion, &estructura);
    let ReglaEvidencia::Implicacion {
        antecedente,
        consecuente,
    } = resultado.evidencia.regla
    else {
        panic!("se esperaba evidencia de implicación");
    };
    assert!(!antecedente.valor);
    assert!(
        consecuente.is_none(),
        "el consecuente no se evalúa al cortocircuitar"
    );
}

#[test]
fn cada_constructor_atomico_se_identifica_sin_guardar_el_texto() {
    let estructura = mundo().estructura;
    let casos = [
        ("(Type @n/root a)", ConstructorAtomico::Type),
        (
            "(Edge @n/root→n/middle#rel#0 @n/root @n/middle rel)",
            ConstructorAtomico::Edge,
        ),
        ("(Parent @n/middle @n/root)", ConstructorAtomico::Parent),
        (
            "(HasLab @n/root \"compartida\")",
            ConstructorAtomico::HasLabNodo,
        ),
        (
            "(HasLab @n/root→n/middle#rel#0 \"arista\")",
            ConstructorAtomico::HasLabArista,
        ),
        ("(Nivel @n/root alto)", ConstructorAtomico::Ext),
        ("(tc Parent @n/leaf @n/root)", ConstructorAtomico::TcParent),
        ("(= @n/root @n/root)", ConstructorAtomico::IgualNodo),
        (
            "(= @n/root→n/middle#rel#0 @n/root→n/middle#rel#0)",
            ConstructorAtomico::IgualArista,
        ),
    ];
    for (texto, esperado) in casos {
        let formula = oracion(texto);
        let resultado = comprobar_resultado(&formula, &estructura);
        let ReglaEvidencia::Atomo {
            constructor,
            camino_tc,
        } = resultado.evidencia.regla
        else {
            panic!("se esperaba evidencia atómica para {texto}");
        };
        assert_eq!(constructor, esperado, "{texto}");
        assert_eq!(
            camino_tc.is_some(),
            esperado == ConstructorAtomico::TcParent
        );
    }

    let conjuncion_vacia = comprobar_resultado(&Formula::Y(Vec::new()), &estructura);
    assert!(conjuncion_vacia.valor);
    assert_eq!(
        conjuncion_vacia.evidencia.regla,
        ReglaEvidencia::Conjuncion {
            partes_evaluadas: Vec::new(),
        }
    );
    let disyuncion_vacia = comprobar_resultado(&Formula::O(Vec::new()), &estructura);
    assert!(!disyuncion_vacia.valor);
    assert_eq!(
        disyuncion_vacia.evidencia.regla,
        ReglaEvidencia::Disyuncion {
            partes_evaluadas: Vec::new(),
        }
    );
}

#[test]
fn cuantificador_universal_entrega_la_primera_asignacion_falsa_y_replay_de_guardas() {
    let estructura = mundo().estructura;
    let formula = oracion("(forall ((x N)) :guarda (Type x b) (Type x a))");
    let resultado = comprobar_resultado(&formula, &estructura);
    let Formula::ParaTodo {
        ligaduras: _,
        guarda,
        cuerpo,
    } = &formula
    else {
        panic!("se esperaba un universal");
    };
    let ReglaEvidencia::Cuantificador {
        tipo,
        instancias,
        busqueda_agotada,
    } = &resultado.evidencia.regla
    else {
        panic!("se esperaba evidencia cuantificada");
    };
    assert_eq!(*tipo, TipoCuantificador::ParaTodo);
    assert!(
        !busqueda_agotada,
        "el falso universal se corta en su testigo"
    );
    assert_eq!(instancias.len(), 2);
    assert_eq!(
        instancias[0].asignacion.ligaduras,
        vec![LigaduraCanonica {
            variable: Var::nuevo("x"),
            valor: ValorLigadura::Nodo(ruta("n/leaf")),
        }]
    );
    assert!(
        instancias[0].valor,
        "guarda falsa: instancia universal vacua"
    );
    assert_eq!(
        instancias[0].cuerpo, None,
        "el cuerpo guardado no se visita"
    );
    assert_eq!(
        instancias[1].asignacion.ligaduras[0].valor,
        ValorLigadura::Nodo(ruta("n/middle"))
    );
    assert!(!instancias[1].valor);
    comprobar_instancias_con_oraculo(guarda.as_deref(), cuerpo, instancias, true, &estructura);
}

#[test]
fn producto_bisortido_y_existencial_usan_asignacion_canonica_reproducible() {
    let estructura = mundo().estructura;
    let formula = oracion(
        "(exists ((e E) (x N)) :guarda (Edge e @n/root x rel) (= e @n/root→n/middle#rel#1))",
    );
    let resultado = comprobar_resultado(&formula, &estructura);
    let Formula::Existe { guarda, cuerpo, .. } = &formula else {
        panic!("se esperaba un existencial");
    };
    let ReglaEvidencia::Cuantificador {
        tipo,
        instancias,
        busqueda_agotada,
    } = &resultado.evidencia.regla
    else {
        panic!("se esperaba evidencia cuantificada");
    };
    assert_eq!(*tipo, TipoCuantificador::Existe);
    assert!(
        !busqueda_agotada,
        "el existencial se corta en el primer testigo"
    );
    assert_eq!(instancias.len(), 10);
    assert_eq!(
        instancias[0].asignacion.ligaduras,
        vec![
            LigaduraCanonica {
                variable: Var::nuevo("e"),
                valor: ValorLigadura::Arista(IdArista::parsear("n/middle→n/leaf#sub#0").unwrap(),),
            },
            LigaduraCanonica {
                variable: Var::nuevo("x"),
                valor: ValorLigadura::Nodo(ruta("n/leaf")),
            },
        ]
    );
    let ultima = instancias.last().unwrap();
    assert!(ultima.valor);
    assert_eq!(
        ultima.asignacion.ligaduras,
        vec![
            LigaduraCanonica {
                variable: Var::nuevo("e"),
                valor: ValorLigadura::Arista(IdArista::parsear("n/root→n/middle#rel#1").unwrap(),),
            },
            LigaduraCanonica {
                variable: Var::nuevo("x"),
                valor: ValorLigadura::Nodo(ruta("n/middle")),
            },
        ]
    );
    comprobar_instancias_con_oraculo(guarda.as_deref(), cuerpo, instancias, false, &estructura);
    assert_eq!(evaluar_trazable(&formula, &estructura).unwrap(), resultado);
    assert!(verificar_arbol_con_oraculo(
        &formula,
        &resultado.evidencia,
        &estructura,
        &AsignacionOraculo::default(),
    ));
}

#[test]
fn cuantificadores_vacios_no_inventan_asignaciones() {
    let estructura = Estructura::nueva(manifiesto_prueba()).unwrap();
    for (texto, esperado, tipo) in [
        (
            "(forall ((x N)) (= x x))",
            true,
            TipoCuantificador::ParaTodo,
        ),
        ("(exists ((x N)) (= x x))", false, TipoCuantificador::Existe),
    ] {
        let formula = oracion(texto);
        let resultado = comprobar_resultado(&formula, &estructura);
        let ReglaEvidencia::Cuantificador {
            tipo: tipo_evidencia,
            instancias,
            busqueda_agotada,
        } = resultado.evidencia.regla
        else {
            panic!("se esperaba evidencia cuantificada");
        };
        assert_eq!(tipo_evidencia, tipo);
        assert_eq!(resultado.valor, esperado);
        assert!(busqueda_agotada);
        assert!(
            instancias.is_empty(),
            "no hay asignación en un dominio vacío"
        );
    }
}

#[test]
fn existencial_falso_registra_agotamiento_sin_elegir_un_falso_testigo() {
    let estructura = mundo().estructura;
    let formula = oracion("(exists ((x N)) (Type x Retirado))");
    let resultado = comprobar_resultado(&formula, &estructura);
    let Formula::Existe { cuerpo, .. } = &formula else {
        panic!("se esperaba un existencial");
    };
    let ReglaEvidencia::Cuantificador {
        tipo,
        instancias,
        busqueda_agotada,
    } = &resultado.evidencia.regla
    else {
        panic!("se esperaba evidencia cuantificada");
    };
    assert_eq!(*tipo, TipoCuantificador::Existe);
    assert!(!resultado.valor);
    assert!(*busqueda_agotada);
    assert_eq!(instancias.len(), estructura.len_nodos());
    assert!(instancias.iter().all(|instancia| !instancia.valor));
    let rutas: Vec<_> = instancias
        .iter()
        .map(|instancia| instancia.asignacion.ligaduras[0].valor.clone())
        .collect();
    assert_eq!(
        rutas,
        ["n/leaf", "n/middle", "n/root", "n/solo"]
            .into_iter()
            .map(|path| ValorLigadura::Nodo(ruta(path)))
            .collect::<Vec<_>>()
    );
    comprobar_instancias_con_oraculo(None, cuerpo, instancias, false, &estructura);
}

#[test]
fn tc_estricto_incluye_ruta_positiva_y_no_inventa_ruta_reflexiva() {
    let estructura = mundo().estructura;
    let falsa = oracion("(not (tc Parent @n/leaf @n/root))");
    let resultado = comprobar_resultado(&falsa, &estructura);
    let ReglaEvidencia::No { interior } = resultado.evidencia.regla else {
        panic!("se esperaba una negación");
    };
    let ReglaEvidencia::Atomo {
        constructor,
        camino_tc,
    } = interior.regla
    else {
        panic!("se esperaba evidencia atómica");
    };
    assert_eq!(constructor, ConstructorAtomico::TcParent);
    assert_eq!(
        camino_tc,
        Some(vec![ruta("n/leaf"), ruta("n/middle"), ruta("n/root")])
    );
    let Formula::No(tc) = &falsa else {
        panic!("se esperaba not");
    };
    assert!(oraculo(tc, &estructura, &mut AsignacionOraculo::default()).unwrap());

    let reflexiva = oracion("(tc Parent @n/leaf @n/leaf)");
    let resultado = comprobar_resultado(&reflexiva, &estructura);
    let ReglaEvidencia::Atomo { camino_tc, .. } = resultado.evidencia.regla else {
        panic!("se esperaba evidencia atómica");
    };
    assert!(!resultado.valor);
    assert!(
        camino_tc.is_none(),
        "tc Parent es estricta y no tiene ruta de cero pasos"
    );
}

#[test]
fn verificador_recursivo_confirma_forall_exists_y_restaura_scopes_hermanos() {
    let estructura = mundo().estructura;
    let formula = oracion(
        "(and (forall ((x N)) (exists ((y N)) (= x y))) \
         (exists ((x N)) (forall ((y N)) (or (= x y) (not (= x y))))) )",
    );
    let resultado = comprobar_resultado(&formula, &estructura);
    assert!(resultado.valor);
    assert!(verificar_arbol_con_oraculo(
        &formula,
        &resultado.evidencia,
        &estructura,
        &AsignacionOraculo::default(),
    ));
}

#[test]
fn error_de_presupuesto_no_publica_evidencia_parcial() {
    let estructura = mundo().estructura;
    let formula = Formula::Y(Vec::new());
    assert_eq!(
        evaluar_trazable_con_presupuesto(&formula, &estructura, 1),
        Err(ErrorEvaluacion::PresupuestoAgotado { limite: 1 })
    );
    let suficiente = evaluar_trazable_con_presupuesto(&formula, &estructura, 2).unwrap();
    assert!(suficiente.valor);
    assert_eq!(
        suficiente.evidencia.regla,
        ReglaEvidencia::Conjuncion {
            partes_evaluadas: Vec::new(),
        }
    );
    assert_eq!(PRESUPUESTO_EVIDENCIA_POR_DEFECTO, 1_000_000);
    assert_eq!(
        evaluar_trazable_con_presupuestos(&formula, &estructura, 2, 1),
        Err(ErrorEvaluacion::PresupuestoEvidenciaAgotado { limite: 1 })
    );
    let suficiente_evidencia =
        evaluar_trazable_con_presupuestos(&formula, &estructura, 2, 2).unwrap();
    assert!(suficiente_evidencia.valor);

    let preflight_invalido = oracion("(or (Type @n/root a) (Type @n/missing a))");
    let esperado = evaluar(&preflight_invalido, &estructura).unwrap_err();
    assert_eq!(
        evaluar_trazable(&preflight_invalido, &estructura),
        Err(esperado),
        "el preflight compartido rechaza incluso ramas que se cortocircuitarían",
    );

    let producto = oracion(
        "(forall ((x N) (e E)) (or (= e @n/root→n/middle#rel#0) (not (= e @n/root→n/middle#rel#0))))",
    );
    assert!(matches!(
        evaluar_trazable_con_presupuesto(&producto, &estructura, 50),
        Err(ErrorEvaluacion::PresupuestoAgotado { limite: 50 })
    ));
    let trazado = evaluar_trazable_con_presupuesto(&producto, &estructura, 100_000).unwrap();
    assert_eq!(trazado.valor, evaluar(&producto, &estructura).unwrap());
    assert_eq!(
        trazado.valor,
        oraculo(&producto, &estructura, &mut AsignacionOraculo::default()).unwrap()
    );
    assert!(matches!(
        evaluar_trazable_con_presupuestos(&producto, &estructura, 100_000, 1),
        Err(ErrorEvaluacion::PresupuestoEvidenciaAgotado { limite: 1 })
    ));

    let tc_positivo = oracion("(tc Parent @n/leaf @n/root)");
    assert!(matches!(
        evaluar_trazable_con_presupuestos(&tc_positivo, &estructura, 1_000, 27),
        Err(ErrorEvaluacion::PresupuestoEvidenciaAgotado { limite: 27 })
    ));
    assert!(evaluar_trazable_con_presupuestos(&tc_positivo, &estructura, 1_000, 28).is_ok());

    let limite_semantico_minimo = (0..=64)
        .find(|limite| evaluar_con_presupuesto(&tc_positivo, &estructura, *limite).is_ok())
        .expect("el fixture debe caber dentro de 64 unidades semánticas");
    assert!(limite_semantico_minimo > 0);
    assert!(matches!(
        evaluar_con_presupuesto(&tc_positivo, &estructura, limite_semantico_minimo - 1),
        Err(ErrorEvaluacion::PresupuestoAgotado { .. })
    ));
    assert!(evaluar_con_presupuesto(&tc_positivo, &estructura, limite_semantico_minimo).is_ok());
    for limite in 0..=limite_semantico_minimo {
        let decision = evaluar_con_presupuesto(&tc_positivo, &estructura, limite);
        let trazado =
            evaluar_trazable_con_presupuestos(&tc_positivo, &estructura, limite, u64::MAX);
        match (decision, trazado) {
            (Ok(valor), Ok(resultado)) => assert_eq!(valor, resultado.valor),
            (Err(decision), Err(trazado)) => assert_eq!(decision, trazado),
            _ => panic!("la captura TC alteró el resultado del presupuesto semántico {limite}"),
        }
    }
}

#[test]
fn presupuesto_global_medido_reporta_consumo_acotado_sin_evidencia_parcial() {
    let estructura = mundo().estructura;
    let formula = oracion("(forall ((testigo N)) (Type testigo a))");
    let amplio = evaluar_trazable_con_presupuesto_total_medido(&formula, &estructura, 1_000_000)
        .expect("el límite amplio debe permitir cálculo y evidencia");
    assert!(!amplio.resultado.valor);

    let total = amplio.unidades_semanticas + amplio.unidades_evidencia;
    assert!(amplio.unidades_semanticas > 0);
    assert!(amplio.unidades_evidencia > 0);

    // Barre todos los límites insuficientes, incluido cero y exacto−1. Cada
    // error expone solo contadores observados; no contiene resultado ni traza.
    for limite in 0..total {
        let error = evaluar_trazable_con_presupuesto_total_medido(&formula, &estructura, limite)
            .expect_err("todo límite inferior al costo medido debe agotarse");
        assert!(matches!(
            &error.error,
            ErrorEvaluacion::PresupuestoAgotado { .. }
                | ErrorEvaluacion::PresupuestoEvidenciaAgotado { .. }
        ));
        assert!(
            error.unidades_consumidas <= limite,
            "el límite {limite} reportó {} unidades",
            error.unidades_consumidas
        );
        assert_eq!(
            error.unidades_consumidas,
            error.unidades_semanticas + error.unidades_evidencia
        );
        let debug = format!("{error:?}");
        let display = error.to_string();
        assert!(!debug.contains("n/root"));
        assert!(!display.contains("n/root"));
    }

    let exacto = evaluar_trazable_con_presupuesto_total_medido(&formula, &estructura, total)
        .expect("el límite exacto debe permitir cálculo y evidencia completos");
    assert_eq!(exacto, amplio);
    assert_eq!(
        exacto.unidades_semanticas + exacto.unidades_evidencia,
        total
    );

    // Los wrappers legacy conservan decisión y evidencia con presupuestos
    // predeterminados o explícitos suficientemente amplios.
    assert_eq!(
        evaluar_trazable(&formula, &estructura).unwrap(),
        amplio.resultado
    );
    assert_eq!(
        evaluar_trazable_con_presupuesto(&formula, &estructura, 1_000_000).unwrap(),
        amplio.resultado
    );
    assert_eq!(
        evaluar_trazable_con_presupuestos(&formula, &estructura, 1_000_000, 1_000_000).unwrap(),
        amplio.resultado
    );
    assert_eq!(
        evaluar_trazable_con_presupuestos_medidos(&formula, &estructura, 1_000_000, 1_000_000)
            .unwrap()
            .resultado,
        amplio.resultado
    );
}

#[test]
fn presupuesto_de_evidencia_cuenta_bytes_utf8_antes_de_clonar_con_limite_exacto() {
    let mut estructura = Estructura::nueva(manifiesto_prueba()).unwrap();
    let texto_path = format!("n/{}秘密", "segmento-privado-".repeat(12));
    let path = BundlePath::nuevo(&texto_path).unwrap();
    estructura
        .insertar_nodo(
            path.clone(),
            crate::tipos::Sorte::nuevo("a"),
            crate::tipos::Etiqueta::nuevo(""),
        )
        .unwrap();
    let nombre_variable = format!("variable{}", "v".repeat(128));
    let variable = Var::nuevo(&nombre_variable);
    let formula = Formula::Existe {
        ligaduras: vec![(variable.clone(), SorteVar::Nodo)],
        guarda: None,
        cuerpo: Box::new(Formula::IgualNodo(
            TerminoNodo::Var(variable),
            TerminoNodo::Constante(path.clone()),
        )),
    };

    let caracteres_path = texto_path.chars().count();
    let bytes_path = texto_path.len();
    assert!(
        bytes_path > caracteres_path,
        "el fixture incluye multibyte UTF-8"
    );
    let limite_exacto =
        7_u64 + u64::try_from(nombre_variable.len()).unwrap() + u64::try_from(bytes_path).unwrap();
    assert_eq!(evaluar(&formula, &estructura), Ok(true));
    assert_eq!(
        evaluar_trazable_con_presupuestos(&formula, &estructura, 10_000, limite_exacto - 1),
        Err(ErrorEvaluacion::PresupuestoEvidenciaAgotado {
            limite: limite_exacto - 1
        })
    );
    assert!(
        evaluar_trazable_con_presupuestos(&formula, &estructura, 10_000, limite_exacto).is_ok(),
        "el valor exacto cubre entradas más bytes clonados"
    );
    let error_path = ErrorEvaluacion::ConstanteNodoInexistente(path.clone());
    let error_variable = ErrorEvaluacion::VariableNoLigada {
        variable: Var::nuevo(&nombre_variable),
        sorte: SorteVar::Nodo,
    };
    assert!(!error_path.to_string().contains(&texto_path));
    assert!(!error_variable.to_string().contains(&nombre_variable));
}

#[test]
fn presupuesto_exacto_cuenta_bytes_de_identidad_de_arista_y_variable() {
    let mut estructura = Estructura::nueva(manifiesto_prueba()).unwrap();
    let path_origen = BundlePath::nuevo(&format!("n/origen{}秘密", "o".repeat(80))).unwrap();
    let path_destino = BundlePath::nuevo(&format!("n/destino{}", "d".repeat(90))).unwrap();
    let origen = estructura
        .insertar_nodo(
            path_origen.clone(),
            crate::tipos::Sorte::nuevo("a"),
            crate::tipos::Etiqueta::nuevo(""),
        )
        .unwrap();
    let destino = estructura
        .insertar_nodo(
            path_destino.clone(),
            crate::tipos::Sorte::nuevo("b"),
            crate::tipos::Etiqueta::nuevo(""),
        )
        .unwrap();
    estructura
        .insertar_arista(
            origen,
            destino,
            crate::tipos::Sorte::nuevo("rel"),
            crate::tipos::Etiqueta::nuevo(""),
        )
        .unwrap();
    let identidad = IdArista {
        origen: path_origen.clone(),
        sorte: crate::tipos::Sorte::nuevo("rel"),
        destino: path_destino.clone(),
        ordinal: 0,
    };
    let error_arista = ErrorEvaluacion::ConstanteAristaInexistente(identidad.clone());
    let nombre_variable = format!("edge{}", "e".repeat(128));
    let variable = Var::nuevo(&nombre_variable);
    let formula = Formula::Existe {
        ligaduras: vec![(variable.clone(), SorteVar::Arista)],
        guarda: None,
        cuerpo: Box::new(Formula::IgualArista(
            TerminoArista::Var(variable),
            TerminoArista::Constante(identidad),
        )),
    };
    let limite_exacto = 7_u64
        + u64::try_from(nombre_variable.len()).unwrap()
        + u64::try_from(path_origen.como_str().len()).unwrap()
        + u64::try_from(path_destino.como_str().len()).unwrap()
        + u64::try_from("rel".len()).unwrap();
    assert_eq!(evaluar(&formula, &estructura), Ok(true));
    assert!(matches!(
        evaluar_trazable_con_presupuestos(&formula, &estructura, 10_000, limite_exacto - 1),
        Err(ErrorEvaluacion::PresupuestoEvidenciaAgotado { limite })
            if limite == limite_exacto - 1
    ));
    assert!(
        evaluar_trazable_con_presupuestos(&formula, &estructura, 10_000, limite_exacto).is_ok()
    );
    assert!(!error_arista.to_string().contains(path_origen.como_str()));
    assert!(!error_arista.to_string().contains(path_destino.como_str()));
}

#[test]
fn debug_de_resultado_y_error_no_expone_identificadores_ni_datos() {
    let estructura = mundo().estructura;
    let formula = oracion("(exists ((clave_privada E)) (HasLab clave_privada \"arista\"))");
    let resultado = evaluar_trazable(&formula, &estructura).unwrap();
    let depurado = format!("{resultado:?}");
    for sensible in ["clave_privada", "n/root", "n/middle", "rel#0", "arista"] {
        assert!(!depurado.contains(sensible), "Debug filtró {sensible:?}");
    }

    let error = ErrorEvaluacion::ConstanteNodoInexistente(ruta("n/root"));
    let depurado_error = format!("{error:?}");
    assert!(!depurado_error.contains("n/root"));

    let path_secreto = ruta("privado/path-secreto");
    let id_secreto =
        IdArista::parsear("privado/origen→privado/destino#relacion-secreta#7").unwrap();
    let valor_nodo = ValorLigadura::Nodo(path_secreto.clone());
    let valor_arista = ValorLigadura::Arista(id_secreto.clone());
    let asignacion = AsignacionCanonica {
        ligaduras: vec![
            LigaduraCanonica {
                variable: Var::nuevo("variable-secreta"),
                valor: valor_nodo.clone(),
            },
            LigaduraCanonica {
                variable: Var::nuevo("otra-variable-secreta"),
                valor: valor_arista.clone(),
            },
        ],
    };
    let instancia = InstanciaCuantificada {
        asignacion: asignacion.clone(),
        guarda: None,
        cuerpo: None,
        valor: true,
    };
    let regla = ReglaEvidencia::Cuantificador {
        tipo: TipoCuantificador::Existe,
        instancias: vec![instancia.clone()],
        busqueda_agotada: false,
    };
    let regla_tc = ReglaEvidencia::Atomo {
        constructor: ConstructorAtomico::TcParent,
        camino_tc: Some(vec![path_secreto.clone()]),
    };
    let evidencia = crate::evaluador::EvidenciaEvaluacion {
        valor: true,
        regla: regla.clone(),
    };
    let resultado_sintetico = ResultadoTrazable {
        valor: true,
        evidencia: evidencia.clone(),
    };
    let representaciones = [
        format!("{valor_nodo:?}"),
        format!("{valor_arista:?}"),
        format!("{:?}", asignacion.ligaduras[0]),
        format!("{asignacion:?}"),
        format!("{instancia:?}"),
        format!("{regla:?}"),
        format!("{regla_tc:?}"),
        format!("{evidencia:?}"),
        format!("{resultado_sintetico:?}"),
    ];
    for representacion in representaciones {
        for sensible in [
            "privado/",
            "relacion-secreta",
            "variable-secreta",
            "path-secreto",
            "destino",
        ] {
            assert!(
                !representacion.contains(sensible),
                "Debug filtró {sensible:?}: {representacion}"
            );
        }
    }

    let id_en_error = IdArista::parsear("privado/origen→privado/destino#secreto#4").unwrap();
    let errores = [
        ErrorEvaluacion::VariableNoLigada {
            variable: Var::nuevo("nombre-secreto"),
            sorte: SorteVar::Nodo,
        },
        ErrorEvaluacion::VariableDeSorteIncorrecta {
            variable: Var::nuevo("nombre-secreto"),
            esperada: SorteVar::Arista,
        },
        ErrorEvaluacion::VariableRedeclarada(Var::nuevo("nombre-secreto")),
        ErrorEvaluacion::ConstanteNodoInexistente(path_secreto),
        ErrorEvaluacion::ConstanteAristaInexistente(id_en_error),
        ErrorEvaluacion::SorteNodoNoDeclarada(crate::tipos::Sorte::nuevo("sorte-secreta")),
        ErrorEvaluacion::SorteAristaNoDeclarada(crate::tipos::Sorte::nuevo("sorte-secreta")),
        ErrorEvaluacion::EtiquetaNoEnUso(crate::tipos::Etiqueta::nuevo("etiqueta-secreta")),
        ErrorEvaluacion::PredicadoNoDeclarado("predicado-secreto".into()),
        ErrorEvaluacion::ConstanteFueraDeDominio {
            predicado: "predicado-secreto".into(),
            constante: "constante-secreta".into(),
        },
        ErrorEvaluacion::MetaNoInstanciada(Var::nuevo("meta-secreta")),
        ErrorEvaluacion::NodoLigadoInexistente(Var::nuevo("nombre-secreto")),
        ErrorEvaluacion::AristaLigadaInexistente(Var::nuevo("nombre-secreto")),
    ];
    for error in errores {
        let representacion = format!("{error:?}");
        let display = error.to_string();
        for sensible in [
            "privado/",
            "secreto",
            "secreta",
            "nombre-secreto",
            "constante-secreta",
            "predicado-secreto",
        ] {
            assert!(
                !representacion.contains(sensible),
                "Debug de error filtró {sensible:?}: {representacion}"
            );
            assert!(
                !display.contains(sensible),
                "Display de error filtró {sensible:?}: {display}"
            );
        }
    }
}
