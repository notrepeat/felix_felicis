//! C1 de punta a punta: `orden = true` en el manifiesto y `(>= P x c)` en reglas.
use std::{fs, process::Command};

#[test]
fn check_evalua_comparaciones_segun_el_orden_del_manifiesto() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    fs::create_dir_all(root.join("n")).unwrap();
    fs::write(
        root.join("n/firme.md"),
        "---\ntype: a\ntitle: Firme\nconfianza: alta\n---\n",
    )
    .unwrap();
    fs::write(
        root.join("n/dudoso.md"),
        "---\ntype: a\ntitle: Dudoso\nconfianza: baja\n---\n",
    )
    .unwrap();
    let manifest = dir.path().join("manifest.toml");
    fs::write(
        &manifest,
        "version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"Retirado\", \"Patrón\"]\n[[predicados]]\nnombre = \"Confianza\"\nclave = \"confianza\"\ndominio = [\"baja\", \"media\", \"alta\"]\norden = true\n[layout]\ndirectorios = [\"n\"]\n",
    )
    .unwrap();
    let check = |regla: &str| {
        let inv = dir.path().join("inv.sx");
        fs::write(&inv, regla).unwrap();
        Command::new(env!("CARGO_BIN_EXE_universo"))
            .args([
                "check",
                root.to_str().unwrap(),
                "--manifiesto",
                manifest.to_str().unwrap(),
                "--invariantes",
                inv.to_str().unwrap(),
            ])
            .output()
            .unwrap()
            .status
            .code()
            .unwrap()
    };
    assert_eq!(check("(>= Confianza @n/firme media)\n"), 0);
    assert_eq!(check("(forall ((x N)) (>= Confianza x media))\n"), 1);
    assert_eq!(check("(forall ((x N)) (>= Confianza x baja))\n"), 0);
}
