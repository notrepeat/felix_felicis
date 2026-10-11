use super::*;

use crate::atomos::{Atomo, Sujeto};
use crate::evaluador::{ValorLigadura, evaluar_trazable};
use crate::formula::{Formula, Sentencia};
use crate::invariante::expansion::{MetricasExpansion, OracionExpandida, ResultadoExpansionFuente};
use crate::manifiesto::tests::base;
use crate::path::BundlePath;
use crate::tipos::{Etiqueta, Sorte};
use crate::veredicto::{
    ErrorVeredicto, ModoExplicacion, ObjetoExplicacion, ResultadoExplicaciones, explicar,
    explicar_con_consumos, explicar_con_presupuesto_y_consumos,
};

const ROOT_PATH: &str = "n/a-PRIVATE_ROOT_CANARY";
const LEAF_PATH: &str = "n/z-PRIVATE_LEAF_CANARY";
const ROOT_LABEL: &str = "PRIVATE_ROOT_LABEL_CANARY";
const APPLICABLE_LABEL: &str = "PRIVATE_APPLICABLE_LABEL_CANARY";

fn path(text: &str) -> BundlePath {
    BundlePath::nuevo(text).unwrap()
}

fn formula(text: &str) -> Formula {
    match crate::sexpr::parsear(text).unwrap() {
        Sentencia::Oracion(formula) => formula,
        Sentencia::Esquema { .. } => panic!("se esperaba una oración"),
    }
}

fn estructura(reverse_insertion: bool) -> Estructura {
    let mut estructura = Estructura::nueva(base()).unwrap();
    let rows = [(ROOT_PATH, ROOT_LABEL), (LEAF_PATH, APPLICABLE_LABEL)];
    let order = if reverse_insertion { [1, 0] } else { [0, 1] };
    let mut root = None;
    let mut leaf = None;
    for index in order {
        let (path_text, label) = rows[index];
        let id = estructura
            .insertar_nodo(path(path_text), Sorte::nuevo("a"), Etiqueta::nuevo(label))
            .unwrap();
        if path_text == ROOT_PATH {
            root = Some(id);
        } else {
            leaf = Some(id);
        }
    }
    estructura
        .asignar_padre(leaf.unwrap(), root.unwrap())
        .unwrap();
    estructura
}

fn estructura_vacia() -> Estructura {
    Estructura::nueva(base()).unwrap()
}

fn estructura_para_forall(reverse_insertion: bool) -> Estructura {
    let mut estructura = Estructura::nueva(base()).unwrap();
    let rows = [
        (ROOT_PATH, ROOT_LABEL),
        ("n/m-PRIVATE_MIDDLE_CANARY", APPLICABLE_LABEL),
        (LEAF_PATH, APPLICABLE_LABEL),
    ];
    let order = if reverse_insertion {
        [2, 1, 0]
    } else {
        [0, 1, 2]
    };
    let mut root = None;
    let mut middle = None;
    let mut leaf = None;
    for index in order {
        let (path_text, label) = rows[index];
        let id = estructura
            .insertar_nodo(path(path_text), Sorte::nuevo("a"), Etiqueta::nuevo(label))
            .unwrap();
        match path_text {
            ROOT_PATH => root = Some(id),
            "n/m-PRIVATE_MIDDLE_CANARY" => middle = Some(id),
            LEAF_PATH => leaf = Some(id),
            _ => unreachable!(),
        }
    }
    estructura
        .asignar_padre(middle.unwrap(), root.unwrap())
        .unwrap();
    estructura
        .asignar_padre(leaf.unwrap(), middle.unwrap())
        .unwrap();
    estructura
}

fn expansion_directa(formula: Formula) -> ResultadoExpansionFuente {
    ResultadoExpansionFuente {
        oraciones: vec![OracionExpandida {
            formula,
            source_index: 0,
            linea: Some(1),
            columna: Some(1),
            instance_index: None,
            sustituciones: Vec::new(),
        }],
        metricas: MetricasExpansion::default(),
    }
}

fn objeto_de_brecha(formula: &Formula, estructura: &Estructura) -> ObjetoExplicacion {
    let expansion = expansion_directa(formula.clone());
    let resultado = explicar(&expansion, estructura, ModoExplicacion::Decidir).unwrap();
    let ResultadoExplicaciones::Decision(Some(objeto)) = resultado else {
        panic!("el fixture debe producir una instancia falsa")
    };
    objeto
}

fn no_hay_testigo_parent_self(estructura: &Estructura) -> bool {
    estructura
        .nodos()
        .all(|(id, _)| estructura.padre(id) != Some(id))
}

fn instancia_parent_sin_hijos(estructura: &Estructura) -> Option<BundlePath> {
    let mut aplicables: Vec<_> = estructura
        .nodos()
        .filter(|(_, nodo)| nodo.etiqueta.como_str() == APPLICABLE_LABEL)
        .collect();
    aplicables.sort_by(|(_, a), (_, b)| a.path.cmp(&b.path));
    aplicables.into_iter().find_map(|(id, nodo)| {
        let tiene_testigo = estructura
            .nodos()
            .any(|(candidato, _)| estructura.padre(candidato) == Some(id));
        (!tiene_testigo).then(|| nodo.path.clone())
    })
}

#[test]
fn exists_falso_no_vacio_resume_el_dominio_y_el_oraculo_no_halla_testigo() {
    let estructura = estructura(false);
    let formula = formula("(exists ((x N)) (Parent x x))");
    let traza = evaluar_trazable(&formula, &estructura).unwrap();
    assert!(!traza.valor);
    assert!(no_hay_testigo_parent_self(&estructura));

    let objeto = objeto_de_brecha(&formula, &estructura);
    assert_eq!(objeto.counterexample.evidencia, traza.evidencia);
    assert!(objeto.core_presencia().is_none());
    let brecha = objeto.gap().expect("Exists falso elegible lleva gap");
    assert_eq!(brecha.clase, ClaseBrecha::ExistenciaSinTestigo);
    assert_eq!(brecha.conteos.nodos_disponibles, 2);
    assert_eq!(brecha.conteos.aristas_disponibles, 0);
    assert_eq!(brecha.conteos.candidatos_existencial_principal_evaluados, 2);
    assert_eq!(brecha.conteos.asignaciones_universales_examinadas, 0);
    assert_eq!(brecha.forma.ligaduras_nodo, 1);
    assert_eq!(brecha.forma.ligaduras_arista, 0);
    assert!(brecha.forma.guarda.is_none());
    assert_eq!(
        brecha.forma.cuerpo,
        FormaPositiva::Atomo(AtomoBrecha::Parent)
    );
    assert!(brecha.asignacion_universal_sensible().is_none());
}

#[test]
fn exists_falso_en_dominio_vacio_registra_cero_asignaciones_y_no_inventa_ids() {
    let estructura = estructura_vacia();
    let formula = formula("(exists ((x N)) (Parent x x))");
    let traza = evaluar_trazable(&formula, &estructura).unwrap();
    assert!(!traza.valor);
    assert!(estructura.nodos().next().is_none());

    let objeto = objeto_de_brecha(&formula, &estructura);
    assert_eq!(objeto.counterexample.evidencia, traza.evidencia);
    let brecha = objeto.gap().expect("el dominio vacío también es elegible");
    assert_eq!(brecha.clase, ClaseBrecha::ExistenciaSinTestigo);
    assert_eq!(brecha.conteos.nodos_disponibles, 0);
    assert_eq!(brecha.conteos.aristas_disponibles, 0);
    assert_eq!(brecha.conteos.candidatos_existencial_principal_evaluados, 0);
    assert_eq!(brecha.asignacion_universal_sensible(), None);
}

fn brecha_para_toda_existe(reverse_insertion: bool) -> (Estructura, Brecha, EvidenciaEvaluacion) {
    let estructura = estructura_para_forall(reverse_insertion);
    let formula = formula(&format!(
        "(forall ((x N)) :guarda (HasLab x \"{APPLICABLE_LABEL}\") (exists ((y N)) (Parent y x)))"
    ));
    let traza = evaluar_trazable(&formula, &estructura).unwrap();
    assert!(!traza.valor);

    // Oráculo independiente del evaluator: la raíz no aplica; el nodo medio
    // sí aplica y tiene hijo; la hoja es la primera instancia aplicable falsa.
    let hoja_sin_hijos = instancia_parent_sin_hijos(&estructura)
        .expect("la única asignación aplicable no tiene testigo Parent");
    assert_eq!(hoja_sin_hijos.como_str(), LEAF_PATH);
    let objeto = objeto_de_brecha(&formula, &estructura);
    assert_eq!(objeto.counterexample.evidencia, traza.evidencia);
    let brecha = objeto.gap().expect("ForAll/Exists falso elegible").clone();
    (estructura, brecha, traza.evidencia)
}

#[test]
fn forall_exists_usa_primera_guarda_verdadera_y_excluye_guardas_falsas() {
    let (estructura, brecha, _) = brecha_para_toda_existe(false);
    assert_eq!(brecha.clase, ClaseBrecha::ParaTodoExisteSinTestigo);
    assert_eq!(brecha.conteos.nodos_disponibles, 3);
    assert_eq!(brecha.conteos.aristas_disponibles, 0);
    assert_eq!(brecha.conteos.asignaciones_universales_examinadas, 3);
    assert_eq!(brecha.conteos.candidatos_existencial_principal_evaluados, 3);
    assert_eq!(brecha.forma.ligaduras_nodo, 1);
    // La forma resume la obligación Exists faltante, no la guarda exterior.
    assert!(brecha.forma.guarda.is_none());
    assert_eq!(
        brecha.forma.cuerpo,
        FormaPositiva::Atomo(AtomoBrecha::Parent)
    );

    let asignacion = brecha
        .asignacion_universal_sensible()
        .expect("la brecha conserva el binding universal aplicable");
    assert_eq!(asignacion.ligaduras.len(), 1);
    let crate::evaluador::ValorLigadura::Nodo(path_asignado) = &asignacion.ligaduras[0].valor
    else {
        panic!("el binding universal usa el dominio de nodos")
    };
    assert_eq!(path_asignado.como_str(), LEAF_PATH);
    assert!(
        estructura.por_path(path_asignado).is_some(),
        "el ID asignado debe existir"
    );

    let root = estructura.por_path(&path(ROOT_PATH)).unwrap();
    let middle = estructura
        .por_path(&path("n/m-PRIVATE_MIDDLE_CANARY"))
        .unwrap();
    let leaf = estructura.por_path(&path(LEAF_PATH)).unwrap();
    assert_eq!(
        estructura.nodo(root).unwrap().etiqueta.como_str(),
        ROOT_LABEL
    );
    assert_eq!(
        estructura.nodo(middle).unwrap().etiqueta.como_str(),
        APPLICABLE_LABEL
    );
    assert!(
        estructura
            .nodos()
            .any(|(candidate, _)| estructura.padre(candidate) == Some(middle))
    );
    assert_eq!(
        estructura.nodo(leaf).unwrap().etiqueta.como_str(),
        APPLICABLE_LABEL
    );
    assert!(
        !estructura
            .nodos()
            .any(|(candidate, _)| estructura.padre(candidate) == Some(leaf))
    );
}

#[test]
fn conteos_asignacion_y_forma_son_deterministas_bajo_orden_de_insercion() {
    let (estructura_a, brecha_a, evidencia_a) = brecha_para_toda_existe(false);
    let (estructura_b, brecha_b, evidencia_b) = brecha_para_toda_existe(true);

    assert_eq!(brecha_a, brecha_b);
    assert_eq!(format!("{brecha_a:?}"), format!("{brecha_b:?}"));
    assert_eq!(evidencia_a, evidencia_b);
    let path_a = &brecha_a.asignacion_universal_sensible().unwrap().ligaduras[0].valor;
    let path_b = &brecha_b.asignacion_universal_sensible().unwrap().ligaduras[0].valor;
    assert_eq!(path_a, path_b);
    if let ValorLigadura::Nodo(path) = path_a {
        assert!(estructura_a.por_path(path).is_some());
        assert!(estructura_b.por_path(path).is_some());
    } else {
        panic!("la asignación universal debe ser un nodo real")
    }
}

#[test]
fn fuera_del_fragmento_no_publica_brecha_y_conserva_contraejemplo_y_core() {
    let estructura = estructura(false);
    assert!(
        estructura
            .nodos()
            .all(|(_, nodo)| nodo.tipo == Sorte::nuevo("a"))
    );
    for texto in [
        "(exists ((x N)) (not (Type x a)))",
        "(exists ((x N)) (and (Parent x x) (=> (Parent x x) (Parent x x))))",
        "(exists ((x N)) (and (Parent x x) (forall ((y N)) (Parent y y))))",
    ] {
        let fuera_del_fragmento = formula(texto);
        let traza = evaluar_trazable(&fuera_del_fragmento, &estructura).unwrap();
        assert!(
            !traza.valor,
            "el caso debe seguir siendo una obligación falsa"
        );
        let objeto = objeto_de_brecha(&fuera_del_fragmento, &estructura);
        assert!(objeto.gap().is_none(), "forma no positiva: {texto}");
        assert!(objeto.core_presencia().is_none());
        assert_eq!(objeto.counterexample.evidencia, traza.evidencia);
    }

    // La nueva clasificación de gaps deja intacto el core positivo ya
    // disponible para el fragmento universal de presencia.
    let presencia = formula(&format!(
        "(forall ((x N)) :guarda (and (Type x a) (HasLab x \"{APPLICABLE_LABEL}\")) (not (and (Parent x @{ROOT_PATH}))))"
    ));
    let traza_presencia = evaluar_trazable(&presencia, &estructura).unwrap();
    assert!(!traza_presencia.valor);
    let objeto_presencia = objeto_de_brecha(&presencia, &estructura);
    assert_eq!(
        objeto_presencia.counterexample.evidencia,
        traza_presencia.evidencia
    );
    assert!(objeto_presencia.gap().is_none());
    let core = objeto_presencia
        .core_presencia()
        .expect("la extracción de presence debe seguir activa");
    assert_eq!(core.atomos_sensibles().len(), 3);
    let requeridos = [
        Atomo::Type {
            sujeto: path(LEAF_PATH),
            sorte: Sorte::nuevo("a"),
        },
        Atomo::HasLab {
            sujeto: Sujeto::Nodo(path(LEAF_PATH)),
            etiqueta: Etiqueta::nuevo(APPLICABLE_LABEL),
        },
        Atomo::Parent {
            hijo: path(LEAF_PATH),
            padre: path(ROOT_PATH),
        },
    ];
    assert!(
        requeridos
            .iter()
            .all(|requerido| core.atomos_sensibles().contains(requerido))
    );
}

#[test]
fn exists_anidado_positivo_se_resume_recursivamente() {
    let estructura = estructura(false);
    let formula = formula("(exists ((x N)) (exists ((y N)) (Parent y y)))");
    let traza = evaluar_trazable(&formula, &estructura).unwrap();
    assert!(!traza.valor);
    assert!(no_hay_testigo_parent_self(&estructura));

    let objeto = objeto_de_brecha(&formula, &estructura);
    assert_eq!(objeto.counterexample.evidencia, traza.evidencia);
    let brecha = objeto.gap().expect("el Exists anidado es positivo");
    assert!(matches!(
        &brecha.forma.cuerpo,
        FormaPositiva::Existe {
            ligaduras_nodo: 1,
            ligaduras_arista: 0,
            guarda: None,
            cuerpo,
        } if **cuerpo == FormaPositiva::Atomo(AtomoBrecha::Parent)
    ));
}

#[test]
fn debug_y_display_de_brecha_redactan_canarios_de_bundle_y_formula() {
    let estructura = estructura(false);
    let formula = formula(
        "(exists ((PRIVATE_VARIABLE_CANARY N)) (Parent PRIVATE_VARIABLE_CANARY PRIVATE_VARIABLE_CANARY))",
    );
    let objeto = objeto_de_brecha(&formula, &estructura);
    let brecha = objeto.gap().unwrap();
    let debug_gap = format!("{brecha:?}");
    let display_gap = brecha.to_string();
    let debug_object = format!("{objeto:?}");
    for canary in [
        ROOT_PATH,
        LEAF_PATH,
        ROOT_LABEL,
        APPLICABLE_LABEL,
        "PRIVATE_VARIABLE_CANARY",
    ] {
        assert!(!debug_gap.contains(canary), "Debug filtró {canary}");
        assert!(!display_gap.contains(canary), "Display filtró {canary}");
        assert!(
            !debug_object.contains(canary),
            "Debug del objeto filtró {canary}"
        );
    }
}

#[test]
fn presupuesto_de_brecha_exacto_pasa_y_exacto_menos_uno_no_publica_parcial() {
    let estructura = estructura(false);
    let formula = formula("(exists ((x N)) (Parent x x))");
    let traza = evaluar_trazable(&formula, &estructura).unwrap();
    let mut holgado = PresupuestoExplicacion::nuevo(1_000_000);
    let esperado = extraer_brecha(&formula, &traza.evidencia, &estructura, &mut holgado)
        .unwrap()
        .expect("el fixture es elegible");
    let exacto = holgado.metricas().unidades_consumidas;
    assert!(exacto > 0);

    let mut presupuesto_exacto = PresupuestoExplicacion::nuevo(exacto);
    let obtenido = extraer_brecha(
        &formula,
        &traza.evidencia,
        &estructura,
        &mut presupuesto_exacto,
    )
    .expect("el límite exacto debe permitir materializar la brecha");
    assert_eq!(obtenido, Some(esperado));
    assert_eq!(presupuesto_exacto.metricas().unidades_consumidas, exacto);

    let mut presupuesto_corto = PresupuestoExplicacion::nuevo(exacto - 1);
    let error = extraer_brecha(
        &formula,
        &traza.evidencia,
        &estructura,
        &mut presupuesto_corto,
    )
    .expect_err("exacto−1 debe fallar atómicamente");
    assert_eq!(
        error,
        ErrorBrecha::PresupuestoAgotado { limite: exacto - 1 }
    );
    assert!(presupuesto_corto.metricas().unidades_consumidas < exacto);

    let expansion = expansion_directa(formula.clone());
    let medido = explicar_con_consumos(&expansion, &estructura, ModoExplicacion::Decidir).unwrap();
    let limite_total = medido.unidades_consumidas;
    let justo = explicar_con_presupuesto_y_consumos(
        &expansion,
        &estructura,
        ModoExplicacion::Decidir,
        limite_total,
    )
    .expect("el presupuesto global medido exacto debe pasar");
    assert_eq!(justo, medido);
    let error_global = explicar_con_presupuesto_y_consumos(
        &expansion,
        &estructura,
        ModoExplicacion::Decidir,
        limite_total - 1,
    )
    .expect_err("exacto−1 no devuelve objetos ni gaps parciales");
    assert!(matches!(
        error_global,
        ErrorVeredicto::PresupuestoAgotado { limite } if limite == limite_total - 1
    ));
}
