#[path = "support/formal_fixture.rs"]
mod fixture;
use std::collections::BTreeSet;
use universo_core::*;
fn cover(f: &Formula, seen: &mut BTreeSet<&'static str>) {
    let name = match f {
        Formula::Type { .. } => "Type",
        Formula::Edge { .. } => "Edge",
        Formula::Parent { .. } => "Parent",
        Formula::HasLab { .. } => "HasLab",
        Formula::Ext { .. } => "Ext",
        Formula::Compara { .. } => "Compara",
        Formula::TcParent { .. } => "TcParent",
        Formula::IgualNodo(..) => "IgualNodo",
        Formula::IgualArista(..) => "IgualArista",
        Formula::No(x) => {
            cover(x, seen);
            "No"
        }
        Formula::Y(xs) => {
            for x in xs {
                cover(x, seen);
            }
            "Y"
        }
        Formula::O(xs) => {
            for x in xs {
                cover(x, seen);
            }
            "O"
        }
        Formula::Implica(a, b) => {
            cover(a, seen);
            cover(b, seen);
            "Implica"
        }
        Formula::ParaTodo { guarda, cuerpo, .. } => {
            if let Some(g) = guarda {
                cover(g, seen);
            }
            cover(cuerpo, seen);
            "ParaTodo"
        }
        Formula::Existe { guarda, cuerpo, .. } => {
            if let Some(g) = guarda {
                cover(g, seen);
            }
            cover(cuerpo, seen);
            "Existe"
        }
    };
    seen.insert(name);
}
#[test]
fn labelled_corpus_exact_verdicts_and_exhaustive_ast_coverage() {
    let base = fixture::fixture();
    let delta = Delta::nuevo([], []).unwrap();
    let mut seen = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut counts = [0; 3];
    for line in include_str!("../../../evaluation/formal_corpus.tsv")
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        let (id, expected, source) = (fields[0], fields[1], fields[2]);
        assert!(ids.insert(id));
        let parsed = parsear_programa(source).unwrap_or_else(|e| panic!("{id}: {e}"));
        match parsear(source).unwrap() {
            Sentencia::Oracion(f) => cover(&f, &mut seen),
            Sentencia::Esquema { cuerpo, .. } => cover(&cuerpo, &mut seen),
        }
        let actual = match proponer(&base, &delta, &parsed) {
            Ok(ResultadoPropuesta::Admitida(_)) => {
                counts[0] += 1;
                "admit"
            }
            Ok(ResultadoPropuesta::RechazadaInvariantes(_)) => {
                counts[1] += 1;
                "reject"
            }
            Err(_) => {
                counts[2] += 1;
                "error"
            }
            other => panic!("{id}: unexpected {other:?}"),
        };
        assert_eq!(actual, expected, "labelled case {id}");
    }
    assert_eq!(seen.len(), 15, "all Formula variants must be represented");
    println!(
        "FORMAL_CORPUS cases={} admit={} reject={} technical_error={} false_admit=0 ast_variants={}",
        ids.len(),
        counts[0],
        counts[1],
        counts[2],
        seen.len()
    );
}
#[test]
fn each_functional_conflict_class_rejects() {
    let base = fixture::fixture();
    let root = fixture::path("n/root");
    let leaf = fixture::path("n/leaf");
    let cases = [
        (
            Atomo::Type {
                sujeto: root.clone(),
                sorte: Sorte::nuevo("b"),
            },
            ClaseConflicto::Type,
        ),
        (
            Atomo::HasLab {
                sujeto: Sujeto::Nodo(root.clone()),
                etiqueta: Etiqueta::nuevo("different"),
            },
            ClaseConflicto::HasLab,
        ),
        (
            Atomo::Parent {
                hijo: leaf,
                padre: root,
            },
            ClaseConflicto::Parent,
        ),
    ];
    for (atom, class) in cases {
        let delta = Delta::nuevo([atom], []).unwrap();
        let ResultadoPropuesta::RechazadaConflictos(result) = proponer(&base, &delta, &[]).unwrap()
        else {
            panic!("false admit/conflict for {class}")
        };
        assert!(result.diagnosticos().iter().any(|d| d.clase() == class));
    }
}
