use crate::estructura::Estructura;
use crate::formula::{
    DominioEsquema, Formula, Sentencia, SorteVar, SujetoTermino, TerminoArista, TerminoConstExt,
    TerminoEtiqueta, TerminoNodo, TerminoSorte, Var,
};
use crate::manifiesto::Manifiesto;
use crate::path::BundlePath;
use crate::sexpr::{SentenciaFuente, parsear_programa};
use crate::tipos::{Etiqueta, Sorte};

use super::{
    CodigoValidacionInvariante as Codigo, ErrorValidacionInvariante, validar_sentencia,
    validar_sentencia_fuente,
};

struct Caso {
    manifiesto: Manifiesto,
    estructura: Estructura,
}

fn caso() -> Caso {
    let manifiesto = crate::manifiesto::tests::base();
    let mut estructura = Estructura::nueva(manifiesto.clone()).unwrap();
    let a = estructura
        .insertar_nodo(
            BundlePath::nuevo("n/a").unwrap(),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("Etiqueta privada A"),
        )
        .unwrap();
    let b = estructura
        .insertar_nodo(
            BundlePath::nuevo("n/b").unwrap(),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("Etiqueta privada B"),
        )
        .unwrap();
    estructura
        .insertar_arista(a, b, Sorte::nuevo("rel"), Etiqueta::nuevo(""))
        .unwrap();
    estructura.asignar_atributo(a, "Nivel", "bajo").unwrap();
    Caso {
        manifiesto,
        estructura,
    }
}

fn errores(sentencia: &Sentencia) -> Vec<ErrorValidacionInvariante> {
    let caso = caso();
    validar_sentencia(sentencia, &caso.manifiesto, &caso.estructura)
}

fn contiene(errores: &[ErrorValidacionInvariante], codigo: Codigo) -> bool {
    errores.iter().any(|error| error.codigo == codigo)
}

fn tipo_nodo(sorte: TerminoSorte) -> Formula {
    Formula::Type {
        sujeto: TerminoNodo::Var(Var::nuevo("x")),
        sorte,
    }
}

#[test]
fn admite_esquema_kv_en_type_y_tipos_reservados() {
    let esquema = Sentencia::Esquema {
        ligaduras: vec![(Var::nuevo("k"), DominioEsquema::SortesNodo)],
        cuerpo: Formula::ParaTodo {
            ligaduras: vec![(Var::nuevo("x"), SorteVar::Nodo)],
            guarda: None,
            cuerpo: Box::new(tipo_nodo(TerminoSorte::Meta(Var::nuevo("k")))),
        },
    };
    assert!(errores(&esquema).is_empty());

    // Los sortes internos siguen siendo miembros de KV, no quedan filtrados
    // por ser reservados.
    let reservado = Sentencia::Oracion(Formula::Type {
        sujeto: TerminoNodo::Constante(BundlePath::nuevo("n/a").unwrap()),
        sorte: TerminoSorte::Literal(Sorte::nuevo("Retirado")),
    });
    assert!(errores(&reservado).is_empty());
}

#[test]
fn admite_ke_lab_y_dom_predicado_en_posiciones_compatibles() {
    let esquema = Sentencia::Esquema {
        ligaduras: vec![
            (Var::nuevo("k"), DominioEsquema::SortesArista),
            (Var::nuevo("l"), DominioEsquema::Etiquetas),
            (
                Var::nuevo("c"),
                DominioEsquema::DominioPredicado("Nivel".into()),
            ),
        ],
        cuerpo: Formula::ParaTodo {
            ligaduras: vec![
                (Var::nuevo("e"), SorteVar::Arista),
                (Var::nuevo("x"), SorteVar::Nodo),
            ],
            guarda: None,
            cuerpo: Box::new(Formula::Y(vec![
                Formula::Edge {
                    arista: TerminoArista::Var(Var::nuevo("e")),
                    origen: TerminoNodo::Var(Var::nuevo("x")),
                    destino: TerminoNodo::Var(Var::nuevo("x")),
                    sorte: TerminoSorte::Meta(Var::nuevo("k")),
                },
                Formula::HasLab {
                    sujeto: SujetoTermino::Arista(TerminoArista::Var(Var::nuevo("e"))),
                    etiqueta: TerminoEtiqueta::Meta(Var::nuevo("l")),
                },
                Formula::Ext {
                    predicado: "Nivel".into(),
                    sujeto: TerminoNodo::Var(Var::nuevo("x")),
                    constante: TerminoConstExt::Meta(Var::nuevo("c")),
                },
            ])),
        },
    };
    assert!(errores(&esquema).is_empty());
}

#[test]
fn rechaza_meta_en_posicion_de_dominio_incompatible() {
    let esquema = Sentencia::Esquema {
        ligaduras: vec![(Var::nuevo("k"), DominioEsquema::SortesArista)],
        cuerpo: Formula::ParaTodo {
            ligaduras: vec![(Var::nuevo("x"), SorteVar::Nodo)],
            guarda: None,
            cuerpo: Box::new(tipo_nodo(TerminoSorte::Meta(Var::nuevo("k")))),
        },
    };
    assert!(contiene(
        &errores(&esquema),
        Codigo::MetaDominioIncompatible
    ));
}

#[test]
fn rechaza_dominio_de_predicado_no_declarado_y_predicado_no_declarado() {
    let esquema = Sentencia::Esquema {
        ligaduras: vec![(
            Var::nuevo("c"),
            DominioEsquema::DominioPredicado("Secreto".into()),
        )],
        cuerpo: Formula::Ext {
            predicado: "Secreto".into(),
            sujeto: TerminoNodo::Constante(BundlePath::nuevo("n/a").unwrap()),
            constante: TerminoConstExt::Meta(Var::nuevo("c")),
        },
    };
    let errores = errores(&esquema);
    assert!(contiene(&errores, Codigo::MetaDominioDesconocido));
    assert!(contiene(&errores, Codigo::PredicadoNoDeclarado));
}

#[test]
fn rechaza_constantes_de_ext_fuera_del_dominio_cerrado() {
    let sentencia = Sentencia::Oracion(Formula::Ext {
        predicado: "Nivel".into(),
        sujeto: TerminoNodo::Constante(BundlePath::nuevo("n/a").unwrap()),
        constante: TerminoConstExt::Literal("SECRETO".into()),
    });
    let errores = errores(&sentencia);
    assert!(contiene(&errores, Codigo::ConstanteExtFueraDeDominio));
    assert!(!errores[0].to_string().contains("SECRETO"));
}

#[test]
fn rechaza_constantes_de_nodo_y_arista_ausentes() {
    let nodo = Sentencia::Oracion(Formula::Type {
        sujeto: TerminoNodo::Constante(BundlePath::nuevo("n/ausente").unwrap()),
        sorte: TerminoSorte::Literal(Sorte::nuevo("Retirado")),
    });
    assert!(contiene(&errores(&nodo), Codigo::ConstanteNodoInexistente));

    let arista = Sentencia::Oracion(Formula::IgualArista(
        TerminoArista::Constante(crate::atomos::IdArista {
            origen: BundlePath::nuevo("n/a").unwrap(),
            sorte: Sorte::nuevo("rel"),
            destino: BundlePath::nuevo("n/no-existe").unwrap(),
            ordinal: 0,
        }),
        TerminoArista::Constante(crate::atomos::IdArista {
            origen: BundlePath::nuevo("n/a").unwrap(),
            sorte: Sorte::nuevo("rel"),
            destino: BundlePath::nuevo("n/no-existe").unwrap(),
            ordinal: 0,
        }),
    ));
    assert!(contiene(
        &errores(&arista),
        Codigo::ConstanteAristaInexistente
    ));
}

#[test]
fn rechaza_sortes_y_variables_libres_o_de_sorte_incorrecta() {
    let sentencia = Sentencia::Oracion(Formula::Type {
        sujeto: TerminoNodo::Var(Var::nuevo("sin-ligar")),
        sorte: TerminoSorte::Literal(Sorte::nuevo("NoDeclarado")),
    });
    let encontrados = errores(&sentencia);
    assert!(contiene(&encontrados, Codigo::VariableLibre));
    assert!(contiene(&encontrados, Codigo::TipoNodoNoDeclarado));

    let arista_en_nodo = Sentencia::Oracion(Formula::ParaTodo {
        ligaduras: vec![(Var::nuevo("v"), SorteVar::Arista)],
        guarda: None,
        cuerpo: Box::new(Formula::Type {
            sujeto: TerminoNodo::Var(Var::nuevo("v")),
            sorte: TerminoSorte::Literal(Sorte::nuevo("a")),
        }),
    });
    assert!(contiene(
        &errores(&arista_en_nodo),
        Codigo::SorteVariableIncorrecta
    ));

    let edge_no_declarada = Sentencia::Oracion(Formula::ParaTodo {
        ligaduras: vec![
            (Var::nuevo("e"), SorteVar::Arista),
            (Var::nuevo("x"), SorteVar::Nodo),
        ],
        guarda: None,
        cuerpo: Box::new(Formula::Edge {
            arista: TerminoArista::Var(Var::nuevo("e")),
            origen: TerminoNodo::Var(Var::nuevo("x")),
            destino: TerminoNodo::Var(Var::nuevo("x")),
            sorte: TerminoSorte::Literal(Sorte::nuevo("NoDeclarado")),
        }),
    });
    assert!(contiene(
        &errores(&edge_no_declarada),
        Codigo::TipoAristaNoDeclarado
    ));
}

#[test]
fn rechaza_metavariables_sin_ligadura_sin_uso_y_duplicadas() {
    let libre = Sentencia::Oracion(tipo_nodo(TerminoSorte::Meta(Var::nuevo("k"))));
    assert!(contiene(&errores(&libre), Codigo::MetaNoLigada));

    let sin_uso = Sentencia::Esquema {
        ligaduras: vec![(Var::nuevo("k"), DominioEsquema::SortesNodo)],
        cuerpo: Formula::O(vec![]),
    };
    assert!(contiene(&errores(&sin_uso), Codigo::MetaSinUso));

    let duplicada = Sentencia::Esquema {
        ligaduras: vec![
            (Var::nuevo("k"), DominioEsquema::SortesNodo),
            (Var::nuevo("k"), DominioEsquema::SortesArista),
        ],
        cuerpo: Formula::ParaTodo {
            ligaduras: vec![(Var::nuevo("x"), SorteVar::Nodo)],
            guarda: None,
            cuerpo: Box::new(tipo_nodo(TerminoSorte::Meta(Var::nuevo("k")))),
        },
    };
    let errores = errores(&duplicada);
    assert!(contiene(&errores, Codigo::MetaDuplicada));
    // Al recuperarse, la primera declaración duplicada conserva su dominio.
    assert!(!contiene(&errores, Codigo::MetaDominioIncompatible));
}

#[test]
fn valida_alcance_y_colision_de_variable_con_meta() {
    let redeclarada = Sentencia::Oracion(Formula::ParaTodo {
        ligaduras: vec![(Var::nuevo("x"), SorteVar::Nodo)],
        guarda: None,
        cuerpo: Box::new(Formula::Existe {
            ligaduras: vec![(Var::nuevo("x"), SorteVar::Nodo)],
            guarda: None,
            cuerpo: Box::new(tipo_nodo(TerminoSorte::Literal(Sorte::nuevo("a")))),
        }),
    });
    assert!(contiene(
        &errores(&redeclarada),
        Codigo::VariableRedeclarada
    ));

    let colision = Sentencia::Esquema {
        ligaduras: vec![(Var::nuevo("x"), DominioEsquema::SortesNodo)],
        cuerpo: Formula::ParaTodo {
            ligaduras: vec![(Var::nuevo("x"), SorteVar::Nodo)],
            guarda: None,
            cuerpo: Box::new(tipo_nodo(TerminoSorte::Literal(Sorte::nuevo("a")))),
        },
    };
    assert!(contiene(
        &errores(&colision),
        Codigo::VariableColisionaConMeta
    ));
}

#[test]
fn rechaza_cambio_de_sorte_entre_alcances_hermanos_y_anidamiento_excesivo() {
    let cambia_de_sorte = Sentencia::Oracion(Formula::Y(vec![
        Formula::ParaTodo {
            ligaduras: vec![(Var::nuevo("v"), SorteVar::Nodo)],
            guarda: None,
            cuerpo: Box::new(Formula::O(vec![])),
        },
        Formula::ParaTodo {
            ligaduras: vec![(Var::nuevo("v"), SorteVar::Arista)],
            guarda: None,
            cuerpo: Box::new(Formula::O(vec![])),
        },
    ]));
    assert!(contiene(
        &errores(&cambia_de_sorte),
        Codigo::VariableCambiaDeSorte
    ));

    let mut al_limite = Formula::O(vec![]);
    for _ in 0..(super::PROFUNDIDAD_MAXIMA_FORMULA - 1) {
        al_limite = Formula::No(Box::new(al_limite));
    }
    assert!(errores(&Sentencia::Oracion(al_limite)).is_empty());

    let mut formula = Formula::O(vec![]);
    for _ in 0..super::PROFUNDIDAD_MAXIMA_FORMULA {
        formula = Formula::No(Box::new(formula));
    }
    assert!(contiene(
        &errores(&Sentencia::Oracion(formula)),
        Codigo::AnidamientoExcesivo
    ));
}

#[test]
fn valida_identificadores_de_ast_directo_sin_rechazar_el_azucar_raiz() {
    let invalido = Sentencia::Oracion(Formula::ParaTodo {
        ligaduras: vec![(Var::nuevo("nombre inválido"), SorteVar::Nodo)],
        guarda: None,
        cuerpo: Box::new(Formula::O(vec![])),
    });
    assert!(contiene(&errores(&invalido), Codigo::IdentificadorInvalido));

    let reservado_forjado = Sentencia::Oracion(Formula::ParaTodo {
        ligaduras: vec![(Var::nuevo("%p0"), SorteVar::Nodo)],
        guarda: None,
        cuerpo: Box::new(Formula::O(vec![])),
    });
    assert!(contiene(
        &errores(&reservado_forjado),
        Codigo::IdentificadorInvalido
    ));

    let raiz = match parsear_programa("(Raiz @n/a)").unwrap().remove(0).sentencia {
        Sentencia::Oracion(formula) => formula,
        Sentencia::Esquema { .. } => unreachable!(),
    };
    assert!(errores(&Sentencia::Oracion(raiz)).is_empty());
}

#[test]
fn aplica_limite_de_64_ligaduras_antes_de_validar_o_expandir() {
    let con_64 = Sentencia::Oracion(Formula::ParaTodo {
        ligaduras: (0..super::MAX_LIGADURAS_POR_CUANTIFICADOR)
            .map(|indice| (Var::nuevo(&format!("v{indice}")), SorteVar::Nodo))
            .collect(),
        guarda: None,
        cuerpo: Box::new(Formula::O(vec![])),
    });
    assert!(errores(&con_64).is_empty());

    let con_65 = Sentencia::Oracion(Formula::ParaTodo {
        ligaduras: (0..=super::MAX_LIGADURAS_POR_CUANTIFICADOR)
            .map(|indice| (Var::nuevo(&format!("v{indice}")), SorteVar::Nodo))
            .collect(),
        guarda: None,
        cuerpo: Box::new(Formula::O(vec![])),
    });
    let errores = errores(&con_65);
    assert_eq!(errores.len(), 1);
    assert_eq!(errores[0].codigo, Codigo::LimiteLigadurasExcedido);
}

#[test]
fn limites_de_validacion_coinciden_con_parser_y_contrato() {
    assert_eq!(
        super::PROFUNDIDAD_MAXIMA_FORMULA,
        crate::sexpr::PROFUNDIDAD_MAXIMA
    );
    assert_eq!(super::PROFUNDIDAD_MAXIMA_FORMULA, 32);
    assert_eq!(super::MAX_LIGADURAS_POR_CUANTIFICADOR, 64);
}

#[test]
fn valida_identificadores_de_metavariables_de_esquema() {
    let esquema = Sentencia::Esquema {
        ligaduras: vec![(Var::nuevo("@meta"), DominioEsquema::SortesNodo)],
        cuerpo: Formula::ParaTodo {
            ligaduras: vec![(Var::nuevo("x"), SorteVar::Nodo)],
            guarda: None,
            cuerpo: Box::new(tipo_nodo(TerminoSorte::Meta(Var::nuevo("@meta")))),
        },
    };
    assert!(contiene(&errores(&esquema), Codigo::IdentificadorInvalido));
}

#[test]
fn detecta_manifiesto_invalido_y_desalineado() {
    let caso = caso();
    let mut invalido = caso.manifiesto.clone();
    invalido.version.clear();
    let sentencia = Sentencia::Oracion(Formula::O(vec![]));
    let errores = validar_sentencia(&sentencia, &invalido, &caso.estructura);
    assert_eq!(errores[0].codigo, Codigo::ManifiestoInvalido);

    let mut distinto = crate::manifiesto::tests::base();
    distinto.version.push_str("/otra");
    let errores = validar_sentencia(&sentencia, &distinto, &caso.estructura);
    assert_eq!(errores[0].codigo, Codigo::ManifiestoDesalineado);
}

#[test]
fn conserva_posicion_de_sentencia_sin_eco_de_datos() {
    let programa = parsear_programa("; nota\n(Type @n/a Privado)").unwrap();
    let fuente: &SentenciaFuente = &programa[0];
    let caso = caso();
    let errores = validar_sentencia_fuente(fuente, &caso.manifiesto, &caso.estructura);
    assert_eq!((errores[0].linea, errores[0].columna), (Some(2), Some(1)));
    assert!(!errores[0].to_string().contains("Privado"));
    assert_eq!(errores[0].ubicacion, "$.formula.sorte");
}
