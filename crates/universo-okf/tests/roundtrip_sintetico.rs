//! Garantía de round-trip sobre un bundle sintético (spec §4.4, entregable
//! 3): RT1 (semántica: átomos y anexos estables) y RT2 (bytes: la
//! serialización es idempotente byte a byte a partir de la segunda vuelta).

use universo_okf::{cargar_bundle, serializar_bundle};
use universo_testkit::{bundle_sintetico::Constructor, manifiesto_minimo};

fn bundle() -> Constructor {
    Constructor::nuevo()
        .archivo("n/index.md", "---\ntype: a\ntitle: Raíz\n---\n")
        .archivo("n/x.md", "---\ntype: a\ntitle: \"X: con dos puntos\"\nnivel: bajo\ngenerated: { by: h, at: t }\nverified:\n  - { by: h, kind: human }\nrel: [\"[[y]]\", \"[[y]]\", \"[[z]]\"]\nsub: [\"[[y]]\"]\nten:\n  - with:  \"[[y]]\"\n    scope: \"uno, dos\"\n---\r\n# X\r\n\r\ncuerpo con crlf\r\n")
        .archivo("n/y.md", "---\r\ntype: b\r\ntitle: Y\r\nrel: [\"[[x]]\", \"[[x]]\"]\r\nsup: [\"[[x]]\"]\r\nten:\r\n  - with: \"[[x]]\"\r\n    scope: \"uno, dos\"\r\n---\r\nsin salto final")
        .archivo("n/sub/index.md", "---\ntype: a\ntitle: Sub\n---\n")
        .archivo("n/sub/z.md", "---\ntype: a\ntitle: Z\nrel: [\"[[x]]\"]\n---\n")
        .archivo("n/libre/acta.md", "---\ntype: a\ntitle: Acta\n---\n")
        .archivo("n/p.md", "---\ntype: pattern\ntitle: P\n---\n")
        .archivo("n/r.md", "---\ntype: a\ntitle: R\nstatus: deprecated\n---\n")
        .archivo("n/log.md", "ignorado")
}

#[test]
fn rt1_atomos_y_anexos_estables() {
    let d0 = bundle().escribir();
    let u0 = cargar_bundle(d0.path(), manifiesto_minimo()).unwrap();
    let d1 = tempfile::tempdir().unwrap();
    serializar_bundle(&u0, d1.path()).unwrap();
    let u1 = cargar_bundle(d1.path(), manifiesto_minimo()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(u0.atomos(), u1.atomos());
    for (id, n) in u0.nodos() {
        let id1 = u1.por_path(&n.path).unwrap();
        assert_eq!(u0.anexo(id), u1.anexo(id1), "anexo de {}", n.path);
    }
    let t: Vec<String> = u1.atomos().iter().map(ToString::to_string).collect();
    assert!(t.contains(&"Edge(n/x→n/y#rel#1, n/x, n/y, rel)".to_string()));
    assert!(t.contains(&"Parent(n/sub/z, n/sub/index)".to_string()));
    assert!(t.contains(&"Type(n/r, Retirado)".to_string()));
    assert!(t.contains(&"Type(n/p, Patrón)".to_string()));
    assert!(!t.iter().any(|a| a.starts_with("Parent(n/libre/")));
    assert_eq!(
        u1.anexo(
            u1.por_path(&universo_core::BundlePath::nuevo("n/y").unwrap())
                .unwrap()
        )
        .unwrap()
        .cuerpo,
        "sin salto final"
    );
}

#[test]
fn rt2_serializacion_idempotente_byte_a_byte() {
    let d0 = bundle().escribir();
    let u0 = cargar_bundle(d0.path(), manifiesto_minimo()).unwrap();
    let d1 = tempfile::tempdir().unwrap();
    serializar_bundle(&u0, d1.path()).unwrap();
    let u1 = cargar_bundle(d1.path(), manifiesto_minimo()).unwrap();
    let d2 = tempfile::tempdir().unwrap();
    serializar_bundle(&u1, d2.path()).unwrap();
    for rel in [
        "n/index.md",
        "n/x.md",
        "n/y.md",
        "n/sub/index.md",
        "n/sub/z.md",
        "n/libre/acta.md",
        "n/p.md",
        "n/r.md",
    ] {
        let a = std::fs::read(d1.path().join(rel)).unwrap();
        let b = std::fs::read(d2.path().join(rel)).unwrap();
        assert_eq!(a, b, "{rel} no es byte-estable");
    }
    assert!(!d1.path().join("n/log.md").exists(), "log.md no se escribe");
    assert!(
        !d1.path().join("n/x.md.tmp").exists(),
        "no quedan temporales"
    );
}

#[test]
fn serializar_sobre_archivo_existente_lo_reemplaza() {
    let d0 = bundle().escribir();
    let u0 = cargar_bundle(d0.path(), manifiesto_minimo()).unwrap();
    serializar_bundle(&u0, d0.path()).unwrap(); // sobre el mismo directorio
    let u1 = cargar_bundle(d0.path(), manifiesto_minimo()).unwrap();
    assert_eq!(u0.atomos(), u1.atomos());
}
