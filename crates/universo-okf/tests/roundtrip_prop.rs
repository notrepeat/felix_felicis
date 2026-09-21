//! Property de round-trip (spec §4.4, plan 10) sobre bundles OKF completos
//! generados por `proptest`: valida RT1 (átomos y anexos estables) y RT2
//! (serialización idempotente byte a byte) sobre una familia mucho más
//! amplia de bundles que los ejemplos escritos a mano de
//! `roundtrip_sintetico.rs`.

use proptest::prelude::*;

use universo_okf::{cargar_bundle, parsear_bundle, serializar_bundle};
use universo_testkit::generadores::bundle_okf;
use universo_testkit::manifiesto_minimo;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn round_trip_de_bundles_okf_generados(archivos in bundle_okf()) {
        // El bundle generado debe ser válido de entrada.
        let u0 = parsear_bundle(&archivos, manifiesto_minimo())
            .unwrap_or_else(|errs| panic!(
                "el bundle generado debería ser válido:\n{}",
                errs.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
            ));

        // ι profundo: `n/index.md` y `n/sub/index.md` pueden coexistir
        // (ninguno es objetivo de wikilink, así que la ambigüedad de stem
        // no aplica) y, cuando ambos están, el segundo cuelga del primero;
        // cualquier nodo bajo `n/sub/` (que no sea el propio índice)
        // cuelga a su vez de `n/sub/index`.
        let atomos0: Vec<String> = u0.atomos().iter().map(ToString::to_string).collect();
        let hay_index = archivos.iter().any(|(p, _)| p == "n/index.md");
        let hay_index_sub = archivos.iter().any(|(p, _)| p == "n/sub/index.md");
        if hay_index && hay_index_sub {
            prop_assert!(
                atomos0.contains(&"Parent(n/sub/index, n/index)".to_string()),
                "falta Parent(n/sub/index, n/index) en {atomos0:?}"
            );
        }
        if hay_index_sub {
            for (path, _) in &archivos {
                let Some(resto) = path.strip_prefix("n/sub/") else {
                    continue;
                };
                let stem = resto.strip_suffix(".md").unwrap_or(resto);
                if stem == "index" {
                    continue;
                }
                prop_assert!(
                    atomos0.contains(&format!("Parent(n/sub/{stem}, n/sub/index)")),
                    "falta Parent(n/sub/{stem}, n/sub/index) en {atomos0:?}"
                );
            }
        }

        // RT1: serializar y recargar conserva átomos y anexos.
        let d1 = tempfile::tempdir().unwrap();
        serializar_bundle(&u0, d1.path()).unwrap();
        let u1 = cargar_bundle(d1.path(), manifiesto_minimo()).unwrap_or_else(|errs| panic!(
            "el bundle serializado debería recargar sin errores:\n{}",
            errs.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
        ));
        prop_assert_eq!(u0.atomos(), u1.atomos(), "RT1: atomos estables");
        for (id, nodo) in u0.nodos() {
            let id1 = u1.por_path(&nodo.path).expect("el path sigue existiendo tras el round-trip");
            prop_assert_eq!(u0.anexo(id), u1.anexo(id1), "RT1: anexo estable de {}", nodo.path);
        }

        // `n/log.md` nunca se escribe: solo se serializan nodos.
        prop_assert!(!d1.path().join("n/log.md").exists());

        // RT2: serializar de nuevo produce los mismos bytes.
        let d2 = tempfile::tempdir().unwrap();
        serializar_bundle(&u1, d2.path()).unwrap();
        for (_, nodo) in u1.nodos() {
            let rel = format!("{}.md", nodo.path);
            let a = std::fs::read(d1.path().join(&rel)).unwrap();
            let b = std::fs::read(d2.path().join(&rel)).unwrap();
            prop_assert_eq!(a, b, "RT2: {} no es byte-estable", rel);
        }
        prop_assert!(!d2.path().join("n/log.md").exists());
    }
}
