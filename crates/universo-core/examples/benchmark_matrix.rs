//! Deterministic in-memory proposal benchmarks; fixture creation/parsing excluded.
use std::{hint::black_box, time::Instant};
use universo_core::*;
fn path(i: usize) -> BundlePath {
    BundlePath::nuevo(&format!("n/item{i}")).unwrap()
}
fn main() {
    let reps: usize = std::env::args()
        .nth(1)
        .map(|s| s.parse().expect("integer repetitions"))
        .unwrap_or(5);
    assert!((1..=30).contains(&reps));
    // One-factor-at-a-time matrix around (32,32,4,1,4), declared before measurement.
    let cases = [
        (32, 32, 4, 1, 4),
        (16, 32, 4, 1, 4),
        (128, 32, 4, 1, 4),
        (512, 32, 4, 1, 4),
        (32, 0, 4, 1, 4),
        (32, 256, 4, 1, 4),
        (32, 32, 1, 1, 4),
        (32, 32, 16, 1, 4),
        (32, 32, 4, 0, 4),
        (32, 32, 4, 2, 4),
        (32, 32, 4, 3, 4),
        (32, 32, 4, 1, 0),
        (32, 32, 4, 1, 16),
        (32, 32, 4, 1, 32),
    ];
    print!(
        "{{\"schema\":\"felix-benchmark-matrix-v1\",\"unit\":\"microseconds\",\"scope\":\"in-memory-proponer-including-candidate\",\"samples\":["
    );
    let mut first = true;
    for (nodes, edges, invariants, variables, delta_nodes) in cases {
        let mut base = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
        let mut ids = vec![];
        for i in 0..nodes {
            ids.push(
                base.insertar_nodo(
                    path(i),
                    Sorte::nuevo("a"),
                    Etiqueta::nuevo(&format!("label{i}")),
                )
                .unwrap(),
            );
        }
        for i in 0..edges {
            base.insertar_arista(
                ids[i % nodes],
                ids[(i + 1) % nodes],
                Sorte::nuevo("rel"),
                Etiqueta::nuevo("edge"),
            )
            .unwrap();
        }
        let mut add = vec![];
        let mut remove = vec![];
        for i in 0..delta_nodes {
            add.push(Atomo::HasLab {
                sujeto: Sujeto::Nodo(path(i)),
                etiqueta: Etiqueta::nuevo(&format!("changed{i}")),
            });
            remove.push(Atomo::HasLab {
                sujeto: Sujeto::Nodo(path(i)),
                etiqueta: Etiqueta::nuevo(&format!("label{i}")),
            });
        }
        let delta = Delta::nuevo(add, remove).unwrap();
        let formula = if variables == 0 {
            "(= @n/item0 @n/item0)".to_string()
        } else {
            let bindings = (0..variables)
                .map(|i| format!("(x{i} N)"))
                .collect::<Vec<_>>()
                .join(" ");
            let terms = (0..variables)
                .map(|i| format!("(= x{i} x{i})"))
                .collect::<Vec<_>>()
                .join(" ");
            format!("(forall ({bindings}) (and {terms}))")
        };
        let program = vec![formula; invariants].join("\n");
        let parsed = parsear_programa(&program).unwrap();
        // One unrecorded warm-up per point; every recorded sample still evaluates fully.
        let _ = outcome(proponer(&base, &delta, &parsed));
        for repetition in 0..reps {
            let start = Instant::now();
            let verdict = black_box(proponer(
                black_box(&base),
                black_box(&delta),
                black_box(&parsed),
            ));
            let elapsed = start.elapsed().as_nanos() as f64 / 1000.0;
            let status = outcome(verdict);
            if !first {
                print!(",");
            }
            first = false;
            print!(
                "{{\"nodes\":{nodes},\"edges\":{edges},\"invariants\":{invariants},\"variables\":{variables},\"delta_atoms\":{},\"repetition\":{},\"elapsed_us\":{elapsed:.3},\"status\":\"{status}\"}}",
                delta_nodes * 2,
                repetition + 1
            );
        }
    }
    println!("]}}");
}

fn outcome(result: Result<ResultadoPropuesta, ErrorPropuesta>) -> &'static str {
    match result {
        Ok(ResultadoPropuesta::Admitida(_)) => "admit",
        Err(error) if error.codigo() == "evaluation_budget_exhausted" => {
            "evaluation_budget_exhausted"
        }
        other => panic!("unexpected benchmark outcome: {other:?}"),
    }
}
