use crate::estructura::Estructura;
use crate::formula::{Formula, Sentencia};
use crate::manifiesto::Manifiesto;
use crate::path::BundlePath;
use crate::sexpr::parsear;
use crate::tipos::{Etiqueta, Sorte};

use super::{
    ErrorExpansion, ValorMeta, expandir_con_presupuesto, expandir_sentencias,
    expandir_sentencias_fuente, expandir_sentencias_fuente_con_presupuesto,
};

fn programa(texto: &str) -> Sentencia {
    parsear(texto).unwrap_or_else(|error| panic!("{texto}: {error}"))
}

fn mundo(reversa: bool) -> (Manifiesto, Estructura) {
    let mut manifiesto = crate::manifiesto::tests::base();
    if reversa {
        manifiesto.sortes_nodo.reverse();
        manifiesto.aristas.reverse();
    }
    let mut estructura = Estructura::nueva(manifiesto.clone()).unwrap();
    let mut a = None;
    let mut b = None;
    for path in if reversa {
        ["n/b", "n/a"]
    } else {
        ["n/a", "n/b"]
    } {
        let (etiqueta, destino) = if path == "n/a" {
            ("omega", &mut a)
        } else {
            ("alpha", &mut b)
        };
        *destino = Some(
            estructura
                .insertar_nodo(
                    BundlePath::nuevo(path).unwrap(),
                    Sorte::nuevo("a"),
                    Etiqueta::nuevo(etiqueta),
                )
                .unwrap(),
        );
    }
    estructura
        .insertar_arista(
            a.unwrap(),
            b.unwrap(),
            Sorte::nuevo("rel"),
            Etiqueta::nuevo("edge-tag"),
        )
        .unwrap();
    (manifiesto, estructura)
}

#[test]
fn expande_los_cuatro_dominios_en_orden_canonico_independiente_de_insercion() {
    let esquema = programa(
        r#"(esquema ((k KV) (e KE) (l Lab) (c Dom.Nivel))
            (forall ((x N) (y N) (a E))
              (and (Type x k) (Edge a x y e) (HasLab x l) (Nivel x c))))"#,
    );
    let (manifiesto_a, estructura_a) = mundo(false);
    let (manifiesto_b, estructura_b) = mundo(true);
    let resultado_a =
        expandir_sentencias(std::slice::from_ref(&esquema), &manifiesto_a, &estructura_a).unwrap();
    let resultado_b =
        expandir_sentencias(std::slice::from_ref(&esquema), &manifiesto_b, &estructura_b).unwrap();

    assert_eq!(resultado_a, resultado_b);
    assert_eq!(resultado_a.metricas.plantillas, 1);
    assert_eq!(resultado_a.metricas.instancias, 54);
    let primero = resultado_a.oraciones[0].to_string();
    assert!(primero.contains("(Type x Patrón)") && primero.contains("(Edge a x y rel)"));
    assert!(primero.contains("(HasLab x \"alpha\")") && primero.contains("(Nivel x alto)"));
    let todas: Vec<String> = resultado_a
        .oraciones
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(todas[1].contains("(Type x Patrón)") && todas[1].contains("(Nivel x bajo)"));
    assert!(todas[2].contains("(HasLab x \"edge-tag\")"));
    assert!(todas[6].contains("(Edge a x y sub)"));
    assert!(todas[18].contains("(Type x Retirado)"));
    for valor in [
        "a", "Patrón", "Retirado", "rel", "sub", "sup", "alpha", "edge-tag", "omega", "alto",
        "bajo",
    ] {
        assert!(todas.iter().any(|f| f.contains(valor)), "falta {valor}");
    }
}

#[test]
fn los_dominios_lab_y_ke_vacios_generan_cero_instancias() {
    let esquema_lab = programa("(esquema ((l Lab)) (forall ((x N)) (HasLab x l)))");
    let manifiesto = crate::manifiesto::tests::base();
    let estructura_vacia = Estructura::nueva(manifiesto.clone()).unwrap();
    let resultado_lab =
        expandir_sentencias(&[esquema_lab], &manifiesto, &estructura_vacia).unwrap();
    assert_eq!(resultado_lab.metricas.instancias, 0);

    let esquema_ke = programa("(esquema ((k KE)) (forall ((e E)) (Edge e @n/a @n/a k)))");
    let mut manifiesto_sin_ke = crate::manifiesto::tests::base();
    manifiesto_sin_ke.aristas.clear();
    let mut estructura_sin_ke = Estructura::nueva(manifiesto_sin_ke.clone()).unwrap();
    estructura_sin_ke
        .insertar_nodo(
            BundlePath::nuevo("n/a").unwrap(),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("label"),
        )
        .unwrap();

    let resultado_ke =
        expandir_sentencias(&[esquema_ke], &manifiesto_sin_ke, &estructura_sin_ke).unwrap();
    assert!(resultado_ke.oraciones.is_empty());
    assert_eq!(resultado_ke.metricas.plantillas, 1);
    assert_eq!(resultado_ke.metricas.instancias, 0);
}

#[test]
fn presupuesto_es_atomico_y_las_oraciones_simples_no_cuentan_como_instancias() {
    let esquema = programa("(esquema ((k KV)) (forall ((x N)) (Type x k)))");
    let sentencia = programa("(Type @n/a a)");
    let (manifiesto, estructura) = mundo(false);
    let programa = [sentencia, esquema];

    assert_eq!(
        expandir_con_presupuesto(&programa, &manifiesto, &estructura, 2),
        Err(ErrorExpansion::PresupuestoAgotado { limite: 2 })
    );
    let exacto = expandir_con_presupuesto(&programa, &manifiesto, &estructura, 3).unwrap();
    assert_eq!(exacto.oraciones.len(), 4);
    assert_eq!(exacto.metricas.plantillas, 1);
    assert_eq!(exacto.metricas.instancias, 3);
}

#[test]
fn valida_todo_antes_de_publicar_oraciones_expandidas() {
    let (manifiesto, estructura) = mundo(false);
    let directa = programa("(Type @n/a a)");
    let invalida = Sentencia::Esquema {
        ligaduras: vec![],
        cuerpo: Formula::Type {
            sujeto: crate::formula::TerminoNodo::Constante(
                BundlePath::nuevo("n/no-existe").unwrap(),
            ),
            sorte: crate::formula::TerminoSorte::Literal(Sorte::nuevo("a")),
        },
    };
    assert!(matches!(
        expandir_sentencias(&[directa.clone(), invalida], &manifiesto, &estructura),
        Err(ErrorExpansion::SentenciaInvalida { indice: 1, .. })
    ));
}

#[test]
fn expansion_conserva_fuente_indices_y_sustituciones_canonicas() {
    let texto = "; línea previa\n(Type @n/a a)\n(esquema ((z Lab) (a Dom.Nivel))\n  (forall ((x N)) (and (HasLab x z) (Nivel x a))))";
    let fuentes = crate::sexpr::parsear_programa(texto).unwrap();
    let (manifiesto, estructura) = mundo(false);
    let resultado = expandir_sentencias_fuente(&fuentes, &manifiesto, &estructura).unwrap();

    assert_eq!(resultado.metricas.plantillas, 1);
    assert_eq!(resultado.metricas.instancias, 6);
    let directa = &resultado.oraciones[0];
    assert_eq!(directa.source_index, 0);
    assert_eq!((directa.linea, directa.columna), (Some(2), Some(1)));
    assert_eq!(directa.instance_index, None);
    assert!(directa.sustituciones.is_empty());

    let primera = &resultado.oraciones[1];
    assert_eq!(primera.source_index, 1);
    assert_eq!((primera.linea, primera.columna), (Some(3), Some(1)));
    assert_eq!(primera.instance_index, Some(0));
    assert_eq!(
        primera
            .sustituciones
            .iter()
            .map(|sustitucion| sustitucion.meta.como_str())
            .collect::<Vec<_>>(),
        ["a", "z"]
    );
    assert_eq!(
        primera.sustituciones[0].valor,
        ValorMeta::ConstanteExt("alto".into())
    );
    assert_eq!(
        primera.sustituciones[1].valor,
        ValorMeta::Etiqueta(Etiqueta::nuevo("alpha"))
    );
    assert_eq!(resultado.oraciones[2].instance_index, Some(1));
    assert_eq!(resultado.oraciones[6].instance_index, Some(5));
}

#[test]
fn expansion_con_fuente_coincide_con_la_api_legacy_y_es_independiente_de_insercion() {
    let texto =
        "(Type @n/a a)\n(esquema ((k KV) (l Lab)) (forall ((x N)) (and (Type x k) (HasLab x l))))";
    let fuentes = crate::sexpr::parsear_programa(texto).unwrap();
    let sentencias: Vec<_> = fuentes
        .iter()
        .map(|fuente| fuente.sentencia.clone())
        .collect();
    let (manifiesto_a, estructura_a) = mundo(false);
    let (manifiesto_b, estructura_b) = mundo(true);
    let con_fuente_a = expandir_sentencias_fuente(&fuentes, &manifiesto_a, &estructura_a).unwrap();
    let con_fuente_b = expandir_sentencias_fuente(&fuentes, &manifiesto_b, &estructura_b).unwrap();
    let legacy = expandir_sentencias(&sentencias, &manifiesto_a, &estructura_a).unwrap();

    assert_eq!(con_fuente_a, con_fuente_b);
    assert_eq!(
        con_fuente_a
            .oraciones
            .iter()
            .map(|oracion| oracion.formula.clone())
            .collect::<Vec<_>>(),
        legacy.oraciones
    );
    assert_eq!(con_fuente_a.metricas, legacy.metricas);
}

#[test]
fn dominio_vacio_con_fuente_no_emite_instancias_y_presupuesto_no_publica_parciales() {
    let esquema =
        crate::sexpr::parsear_programa("(esquema ((k KE)) (forall ((e E)) (Edge e @n/a @n/a k)))")
            .unwrap();
    let mut manifiesto = crate::manifiesto::tests::base();
    manifiesto.aristas.clear();
    let mut estructura = Estructura::nueva(manifiesto.clone()).unwrap();
    estructura
        .insertar_nodo(
            BundlePath::nuevo("n/a").unwrap(),
            Sorte::nuevo("a"),
            Etiqueta::nuevo("label"),
        )
        .unwrap();
    let vacio = expandir_sentencias_fuente(&esquema, &manifiesto, &estructura).unwrap();
    assert!(vacio.oraciones.is_empty());
    assert_eq!(vacio.metricas.plantillas, 1);
    assert_eq!(vacio.metricas.instancias, 0);

    let programa =
        crate::sexpr::parsear_programa("(esquema ((k KV)) (forall ((x N)) (Type x k)))").unwrap();
    let (manifiesto, estructura) = mundo(false);
    assert_eq!(
        expandir_sentencias_fuente_con_presupuesto(&programa, &manifiesto, &estructura, 2),
        Err(ErrorExpansion::PresupuestoAgotado { limite: 2 })
    );
}

#[test]
fn errores_de_expansion_no_interpolan_identificadores_de_dominio() {
    let error = ErrorExpansion::DominioPredicadoDesconocido("dato-privado".into());
    assert!(!error.to_string().contains("dato-privado"));
    assert!(!format!("{error:?}").contains("dato-privado"));
}
