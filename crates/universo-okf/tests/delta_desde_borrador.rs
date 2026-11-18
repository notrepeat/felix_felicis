//! `delta_desde_borrador`: el delta que lleva el root admitido al borrador.
use std::{fs, path::Path};
use universo_core::construir_candidato;
use universo_okf::{cargar_bundle, delta_desde_borrador, serializar_delta_json_canonico};

fn escribir(raiz: &Path, archivos: &[(&str, &str)]) {
    for (ruta, texto) in archivos {
        let destino = raiz.join(ruta);
        fs::create_dir_all(destino.parent().unwrap()).unwrap();
        fs::write(destino, texto).unwrap();
    }
}

const BASE: &[(&str, &str)] = &[
    ("n/index.md", "---\ntype: a\ntitle: Índice\n---\n"),
    (
        "n/x.md",
        "---\ntype: a\ntitle: X\nnivel: bajo\n---\nprosa vieja\n",
    ),
];

#[test]
fn calcula_notas_hechos_y_cuerpos_y_reproduce_el_borrador() {
    let dir = tempfile::tempdir().unwrap();
    let (raiz, borrador) = (dir.path().join("raiz"), dir.path().join("borrador"));
    escribir(&raiz, BASE);
    escribir(
        &borrador,
        &[
            ("n/index.md", "---\ntype: a\ntitle: Índice\n---\n"),
            (
                "n/x.md",
                "---\ntype: b\ntitle: X\nnivel: alto\n---\nprosa nueva\n",
            ),
            (
                "n/y.md",
                "---\ntype: a\ntitle: Y\nnivel: bajo\nrel: [\"[[x]]\"]\n---\nnota nueva\n",
            ),
        ],
    );
    let manifiesto = universo_testkit::manifiesto_minimo();
    let delta = delta_desde_borrador(&raiz, &borrador, manifiesto.clone()).unwrap();
    assert_eq!(delta.crear().len(), 1);
    assert_eq!(delta.cuerpos().len(), 1);
    assert!(!delta.agregar().is_empty() && !delta.retirar().is_empty());
    let texto = serializar_delta_json_canonico(&delta).unwrap();
    assert!(
        texto.starts_with(r#"{"schema":"felix-delta-v2""#),
        "{texto}"
    );
    let base = cargar_bundle(&raiz, manifiesto.clone()).unwrap();
    let esperado = cargar_bundle(&borrador, manifiesto).unwrap();
    let candidato = construir_candidato(&base, &delta).unwrap();
    let mut a = candidato.estructura().atomos();
    let mut b = esperado.atomos();
    a.sort();
    b.sort();
    assert_eq!(a, b);
}

#[test]
fn sin_cambios_da_un_delta_vacio_en_v1() {
    let dir = tempfile::tempdir().unwrap();
    let (raiz, borrador) = (dir.path().join("raiz"), dir.path().join("borrador"));
    escribir(&raiz, BASE);
    escribir(&borrador, BASE);
    let delta =
        delta_desde_borrador(&raiz, &borrador, universo_testkit::manifiesto_minimo()).unwrap();
    assert_eq!(
        serializar_delta_json_canonico(&delta).unwrap(),
        r#"{"schema":"felix-delta-v1","add":[],"remove":[]}"#
    );
}

#[test]
fn borrar_notas_o_tocar_frontmatter_no_mapeado_se_rechaza() {
    let manifiesto = universo_testkit::manifiesto_minimo();
    let dir = tempfile::tempdir().unwrap();
    let (raiz, borrador) = (dir.path().join("raiz"), dir.path().join("borrador"));
    escribir(&raiz, BASE);
    escribir(&borrador, &BASE[..1]);
    assert_eq!(
        delta_desde_borrador(&raiz, &borrador, manifiesto.clone())
            .unwrap_err()
            .codigo(),
        "note_deletion_unsupported"
    );
    let dir = tempfile::tempdir().unwrap();
    let (raiz, borrador) = (dir.path().join("raiz"), dir.path().join("borrador"));
    escribir(&raiz, BASE);
    escribir(
        &borrador,
        &[
            BASE[0],
            (
                "n/x.md",
                "---\ntype: a\ntitle: X\nnivel: bajo\nextra: 1\n---\nprosa vieja\n",
            ),
        ],
    );
    assert_eq!(
        delta_desde_borrador(&raiz, &borrador, manifiesto)
            .unwrap_err()
            .codigo(),
        "passthrough_change_unsupported"
    );
}

fn par(
    base: &[(&str, &str)],
    editado: &[(&str, &str)],
) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let (raiz, borrador) = (dir.path().join("raiz"), dir.path().join("borrador"));
    escribir(&raiz, base);
    escribir(&borrador, editado);
    (dir, raiz, borrador)
}

#[test]
fn archivos_que_no_son_notas_deben_coincidir() {
    let manifiesto = universo_testkit::manifiesto_minimo();
    for extra in [
        ("n/foo.txt", "x"),
        ("n/log.md", "ignorado"),
        ("otro/z.md", "---\ntype: a\ntitle: Z\n---\n"),
    ] {
        let mut editado = BASE.to_vec();
        editado.push(extra);
        let (_d, raiz, borrador) = par(BASE, &editado);
        assert_eq!(
            delta_desde_borrador(&raiz, &borrador, manifiesto.clone())
                .unwrap_err()
                .codigo(),
            "draft_unmanaged_change",
            "{}",
            extra.0
        );
    }
}

#[test]
fn errores_del_nucleo_se_propagan_con_su_codigo() {
    let mut editado = BASE.to_vec();
    editado.push(("n/sub/index.md", "---\ntype: a\ntitle: Sub\n---\n"));
    let (_d, raiz, borrador) = par(BASE, &editado);
    let codigo = delta_desde_borrador(&raiz, &borrador, universo_testkit::manifiesto_minimo())
        .unwrap_err()
        .codigo();
    assert_eq!(codigo, "note_index_not_supported");
}

#[test]
fn anadir_o_quitar_la_clave_explicita_de_padre_se_rechaza() {
    let mut manifiesto = universo_testkit::manifiesto_minimo();
    manifiesto.layout.parent_key = Some("parent".into());
    let sin = (
        "n/x.md",
        "---\ntype: a\ntitle: X\nnivel: bajo\n---\nprosa vieja\n",
    );
    let con = (
        "n/x.md",
        "---\ntype: a\ntitle: X\nnivel: bajo\nparent: n/index\n---\nprosa vieja\n",
    );
    for (base, editado) in [
        ([BASE[0], sin], [BASE[0], con]),
        ([BASE[0], con], [BASE[0], sin]),
    ] {
        let (_d, raiz, borrador) = par(&base, &editado);
        assert_eq!(
            delta_desde_borrador(&raiz, &borrador, manifiesto.clone())
                .unwrap_err()
                .codigo(),
            "parent_key_change_unsupported"
        );
    }
}
