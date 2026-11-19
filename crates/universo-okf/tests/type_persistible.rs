use universo_core::{Atomo, BundlePath, Delta, Sorte, construir_candidato};
use universo_okf::{parsear_bundle, serializar_candidato};

#[test]
fn cambio_type_serializa_alias_y_preserva_atomos_y_anexos_en_roundtrip() {
    let mut manifest = universo_testkit::manifiesto_minimo();
    manifest
        .sortes_nodo_disco
        .insert(Sorte::nuevo("b"), "alias-b".into());
    let docs = vec![
        (
            "n/index.md".into(),
            "---\ntype: a\ntitle: Root\n---\nRoot body\n".into(),
        ),
        (
            "n/leaf.md".into(),
            "---\ntype: a\ntitle: Leaf\ncustom: literal\n---\nPRIVATE_BODY\n".into(),
        ),
    ];
    let snapshot = parsear_bundle(&docs, manifest.clone()).unwrap();
    let path = BundlePath::nuevo("n/leaf").unwrap();
    let delta = Delta::nuevo(
        [Atomo::Type {
            sujeto: path.clone(),
            sorte: Sorte::nuevo("b"),
        }],
        [Atomo::Type {
            sujeto: path.clone(),
            sorte: Sorte::nuevo("a"),
        }],
    )
    .unwrap();
    let candidate = construir_candidato(&snapshot, &delta).unwrap();
    let serialized = serializar_candidato(&snapshot, &candidate, &[]).unwrap();
    let leaf = serialized.iter().find(|d| d.path() == &path).unwrap();
    assert!(leaf.contenido().contains("type: alias-b"));
    let files = serialized
        .iter()
        .map(|d| (format!("{}.md", d.path()), d.contenido().to_owned()))
        .collect::<Vec<_>>();
    let reloaded = parsear_bundle(&files, manifest).unwrap();
    assert_eq!(reloaded.atomos(), candidate.estructura().atomos());
    for (id, node) in snapshot.nodos() {
        assert_eq!(
            snapshot.anexo(id),
            reloaded.anexo(reloaded.por_path(&node.path).unwrap())
        );
    }
    let empty = Delta::nuevo([], []).unwrap();
    let again = construir_candidato(&reloaded, &empty).unwrap();
    let second = serializar_candidato(&reloaded, &again, &[]).unwrap();
    assert_eq!(serialized, second);
    assert_eq!(
        snapshot
            .nodo(snapshot.por_path(&path).unwrap())
            .unwrap()
            .tipo,
        Sorte::nuevo("a")
    );
}
