//! Fase 10, U4: las plantillas de `candidatos` y `vigente` que deja
//! `universo init --git`. Cada una pasa con una nota de ejemplo de cada tipo,
//! y cada regla tiene un caso que la cumple y otro que la incumple (sólo a
//! ella).
use std::{collections::BTreeSet, fs, path::Path};
use universo_core::{Delta, ResultadoPropuesta};
use universo_okf::{
    registro_criterio::{INVARIANTES_CANDIDATOS, INVARIANTES_VIGENTE, MANIFIESTO_CRITERIO},
    transaccion::simulacion::VistaSimulacion,
};

/// Ids de las reglas que fallan con `notas` (ruta relativa → contenido).
fn fallan(invariantes: &str, notas: &[(&str, &str)]) -> BTreeSet<String> {
    let dir = tempfile::tempdir().unwrap();
    let raiz = dir.path().join("bundle");
    for d in ["criterios", "evidencias", "proyectos"] {
        fs::create_dir_all(raiz.join(d)).unwrap();
    }
    for (ruta, texto) in notas {
        fs::write(raiz.join(ruta), texto).unwrap();
    }
    let m = dir.path().join("manifest.toml");
    let i = dir.path().join("invariants.sx");
    fs::write(&m, MANIFIESTO_CRITERIO).unwrap();
    fs::write(&i, invariantes).unwrap();
    veredicto(&raiz, &m, &i)
}

fn veredicto(raiz: &Path, m: &Path, i: &Path) -> BTreeSet<String> {
    let vista = VistaSimulacion::capturar(raiz, m, i).unwrap();
    match vista
        .proponer(&Delta::nuevo(vec![], vec![]).unwrap())
        .unwrap()
    {
        ResultadoPropuesta::Admitida(_) => BTreeSet::new(),
        ResultadoPropuesta::RechazadaInvariantes(fuentes) => fuentes
            .iter()
            .map(|p| {
                vista.invariantes()[p.source_index]
                    .id
                    .clone()
                    .unwrap_or_default()
            })
            .collect(),
        ResultadoPropuesta::RechazadaConflictos(_) => panic!("conflictos"),
    }
}

fn ids(lista: &[&str]) -> BTreeSet<String> {
    lista.iter().map(|s| s.to_string()).collect()
}

const ALFA: (&str, &str) = (
    "proyectos/proyecto-alfa.md",
    "---\ntype: proyecto\ntitle: Alfa\n---\n",
);
const BETA: (&str, &str) = (
    "proyectos/proyecto-beta.md",
    "---\ntype: proyecto\ntitle: Beta\n---\n",
);

fn criterio(estado: &str, proyectos: &str) -> (&'static str, String) {
    (
        "criterios/regla.md",
        format!(
            "---\ntype: criterio\ntitle: Regla\nestado: {estado}\nvalidacion: explicita\nfecha: 2026-11-18\nconfirmado_en: [{proyectos}]\n---\nRegla\n"
        ),
    )
}

fn evidencia(extra: &str) -> (&'static str, String) {
    (
        "evidencias/evidencia.md",
        format!(
            "---\ntype: evidencia\ntitle: Evidencia\nvalidacion: explicita\nfecha: 2026-11-19\napoya: [\"[[regla]]\"]\n{extra}---\n"
        ),
    )
}

fn fallan_con(
    invariantes: &str,
    fijas: &[(&str, &str)],
    notas: &[(&str, String)],
) -> BTreeSet<String> {
    let mut todas: Vec<(&str, &str)> = fijas.to_vec();
    todas.extend(notas.iter().map(|(r, t)| (*r, t.as_str())));
    fallan(invariantes, &todas)
}

const EN_ALFA: &str = "\"[[proyecto-alfa]]\"";
const EN_ALFA_Y_BETA: &str = "\"[[proyecto-alfa]]\", \"[[proyecto-beta]]\"";
const EV_EN_ALFA: &str = "en_proyecto: [\"[[proyecto-alfa]]\"]\n";

#[test]
fn candidatos_admite_un_ejemplo_de_cada_tipo() {
    let f = fallan_con(
        INVARIANTES_CANDIDATOS,
        &[ALFA],
        &[criterio("candidato", EN_ALFA), evidencia(EV_EN_ALFA)],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn vigente_admite_un_ejemplo_de_cada_tipo() {
    let f = fallan_con(
        INVARIANTES_VIGENTE,
        &[ALFA, BETA],
        &[criterio("vigente", EN_ALFA_Y_BETA), evidencia(EV_EN_ALFA)],
    );
    assert!(f.is_empty(), "{f:?}");
    // Un retirado no necesita dos confirmaciones.
    let f = fallan_con(
        INVARIANTES_VIGENTE,
        &[ALFA],
        &[criterio("retirado", EN_ALFA)],
    );
    assert!(f.is_empty(), "{f:?}");
}

/// Las reglas compartidas fallan solas, en las dos plantillas.
#[test]
fn cada_regla_compartida_falla_sola() {
    for (plantilla, estado, proyectos, fijas) in [
        (INVARIANTES_CANDIDATOS, "candidato", EN_ALFA, vec![ALFA]),
        (
            INVARIANTES_VIGENTE,
            "vigente",
            EN_ALFA_Y_BETA,
            vec![ALFA, BETA],
        ),
    ] {
        let base = criterio(estado, proyectos);
        // tipo-conocido: una sorte que el manifiesto declara y la regla no admite.
        let f = fallan_con(
            plantilla,
            &fijas,
            &[
                base.clone(),
                (
                    "criterios/otra.md",
                    "---\ntype: Retirado\ntitle: Otra\n---\n".into(),
                ),
            ],
        );
        assert_eq!(f, ids(&["tipo-conocido"]), "{estado}");

        // criterio-con-contexto: le falta cada uno de sus datos.
        let estado_linea = format!("estado: {estado}\n");
        for quitar in [
            estado_linea.as_str(),
            "validacion: explicita\n",
            "fecha: 2026-11-18\n",
        ] {
            let incompleto = ("criterios/regla.md", base.1.replace(quitar, ""));
            let f = fallan_con(plantilla, &fijas, &[incompleto]);
            // En vigente, sin estado tampoco es un criterio probado.
            let esperado: &[&str] = if estado == "vigente" && quitar == estado_linea {
                &["criterio-con-contexto", "solo-criterio-probado"]
            } else {
                &["criterio-con-contexto"]
            };
            assert_eq!(f, ids(esperado), "{estado}: {quitar}");
        }
        let sin_proyecto = (
            "criterios/regla.md",
            base.1
                .replace(&format!("confirmado_en: [{proyectos}]\n"), ""),
        );
        let f = fallan_con(plantilla, &fijas, &[sin_proyecto]);
        // En vigente, sin proyectos tampoco está confirmado.
        let esperado: &[&str] = if estado == "vigente" {
            &["criterio-con-contexto", "vigente-confirmado"]
        } else {
            &["criterio-con-contexto"]
        };
        assert_eq!(f, ids(esperado), "{estado}");

        // evidencia-con-contexto: sin proyecto, sin fecha, o sin apoyar ni
        // objetar; objetar en vez de apoyar la cumple.
        let f = fallan_con(plantilla, &fijas, &[base.clone(), evidencia("")]);
        assert_eq!(f, ids(&["evidencia-con-contexto"]), "{estado}");
        let sin_fecha = (
            "evidencias/evidencia.md",
            evidencia(EV_EN_ALFA).1.replace("fecha: 2026-11-19\n", ""),
        );
        let f = fallan_con(plantilla, &fijas, &[base.clone(), sin_fecha]);
        assert_eq!(f, ids(&["evidencia-con-contexto"]), "{estado}");
        let sin_criterio = (
            "evidencias/evidencia.md",
            evidencia(EV_EN_ALFA)
                .1
                .replace("apoya: [\"[[regla]]\"]\n", ""),
        );
        let f = fallan_con(plantilla, &fijas, &[base.clone(), sin_criterio]);
        assert_eq!(f, ids(&["evidencia-con-contexto"]), "{estado}");
        let objeta = (
            "evidencias/evidencia.md",
            evidencia(EV_EN_ALFA).1.replace("apoya:", "objeta:"),
        );
        let f = fallan_con(plantilla, &fijas, &[base.clone(), objeta]);
        assert!(f.is_empty(), "{estado}: {f:?}");

        // aristas-bien-tipadas: cada clave con un extremo del tipo equivocado.
        let mal_tipadas = [
            evidencia("en_proyecto: [\"[[proyecto-alfa]]\", \"[[regla]]\"]\n"),
            (
                "criterios/regla.md",
                base.1.replace(
                    &format!("confirmado_en: [{proyectos}]"),
                    &format!("confirmado_en: [{proyectos}, \"[[regla]]\"]"),
                ),
            ),
            (
                "proyectos/proyecto-gamma.md",
                "---\ntype: proyecto\ntitle: Gamma\napoya: [\"[[regla]]\"]\n---\n".into(),
            ),
            (
                "proyectos/proyecto-gamma.md",
                "---\ntype: proyecto\ntitle: Gamma\nobjeta: [\"[[regla]]\"]\n---\n".into(),
            ),
        ];
        for mal in mal_tipadas {
            let ruta = mal.0;
            let notas = if ruta == "criterios/regla.md" {
                vec![mal]
            } else {
                vec![base.clone(), mal]
            };
            let f = fallan_con(plantilla, &fijas, &notas);
            assert_eq!(f, ids(&["aristas-bien-tipadas"]), "{estado}: {ruta}");
        }
    }
}

#[test]
fn las_reglas_propias_de_vigente_fallan_solas() {
    // solo-criterio-probado: una candidata en vigente (aunque esté confirmada).
    let f = fallan_con(
        INVARIANTES_VIGENTE,
        &[ALFA, BETA],
        &[criterio("candidato", EN_ALFA_Y_BETA)],
    );
    assert_eq!(f, ids(&["solo-criterio-probado"]));
    // vigente-confirmado: un solo proyecto.
    let f = fallan_con(
        INVARIANTES_VIGENTE,
        &[ALFA],
        &[criterio("vigente", EN_ALFA)],
    );
    assert_eq!(f, ids(&["vigente-confirmado"]));
    // En candidatos, las dos situaciones son válidas.
    let f = fallan_con(
        INVARIANTES_CANDIDATOS,
        &[ALFA],
        &[criterio("vigente", EN_ALFA)],
    );
    assert!(f.is_empty(), "{f:?}");
}

/// Cada regla de las plantillas está cubierta por algún caso de arriba.
#[test]
fn ninguna_regla_queda_sin_caso() {
    let cubiertas = ids(&[
        "tipo-conocido",
        "criterio-con-contexto",
        "evidencia-con-contexto",
        "aristas-bien-tipadas",
        "solo-criterio-probado",
        "vigente-confirmado",
    ]);
    let mut vistas = 0;
    for plantilla in [INVARIANTES_CANDIDATOS, INVARIANTES_VIGENTE] {
        for linea in plantilla.lines() {
            if let Some(resto) = linea.strip_prefix("(regla :id \"") {
                let id = resto.split('"').next().unwrap();
                assert!(cubiertas.contains(id), "regla sin caso: {id}");
                vistas += 1;
            }
        }
    }
    assert_eq!(vistas, 10);
}
