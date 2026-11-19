//! U2: editar un borrador, calcular el delta, simularlo y aplicarlo.
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn universo(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn copiar(origen: &Path, destino: &Path) {
    fs::create_dir_all(destino).unwrap();
    for entrada in fs::read_dir(origen).unwrap() {
        let entrada = entrada.unwrap();
        let ruta = entrada.path();
        if ruta.is_dir() {
            copiar(&ruta, &destino.join(entrada.file_name()));
        } else {
            fs::copy(&ruta, destino.join(entrada.file_name())).unwrap();
        }
    }
}

#[test]
fn borrador_editado_se_convierte_en_delta_aplicado_y_sellado() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    assert_eq!(universo(&["init", repo.to_str().unwrap()]).0, 0);
    let raiz = repo.join("bundle");
    let borrador = tmp.path().join("borrador");
    copiar(&raiz, &borrador);
    let nota = borrador.join("notas/bienvenida.md");
    let texto = fs::read_to_string(&nota)
        .unwrap()
        .replace("Primera nota", "Nota editada");
    fs::write(&nota, texto).unwrap();
    fs::write(
        borrador.join("notas/fuente.md"),
        "---\ntype: fuente\ntitle: Una fuente\napoya: [\"[[bienvenida]]\"]\n---\nReferencia.\n",
    )
    .unwrap();
    let manifiesto = repo.join("manifest.toml");
    let invariantes = repo.join("invariants.sx");
    let (code, delta, err) = universo(&[
        "delta",
        raiz.to_str().unwrap(),
        "--desde",
        borrador.to_str().unwrap(),
        "--manifiesto",
        manifiesto.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{err}");
    let documento: Value = serde_json::from_str(&delta).unwrap();
    assert_eq!(documento["schema"], "felix-delta-v2");
    assert_eq!(documento["create"][0]["path"], "notas/fuente");
    assert_eq!(documento["update"][0]["path"], "notas/bienvenida");
    let archivo = tmp.path().join("delta.json");
    fs::write(&archivo, &delta).unwrap();
    let base = [
        "propose",
        raiz.to_str().unwrap(),
        "--manifiesto",
        manifiesto.to_str().unwrap(),
        "--invariantes",
        invariantes.to_str().unwrap(),
        "--delta",
        archivo.to_str().unwrap(),
        "--format",
        "json",
    ];
    let mut args = base.to_vec();
    args.push("--emit-snapshot-token");
    let (code, out, _) = universo(&args);
    assert_eq!(code, 0, "{out}");
    let token = serde_json::from_str::<Value>(&out).unwrap()["snapshot_token"]
        .as_str()
        .unwrap()
        .to_string();
    let mut args = base.to_vec();
    args.extend(["--apply", "--expect-snapshot", &token]);
    let (code, out, _) = universo(&args);
    assert_eq!(code, 0, "{out}");
    let (code, out, _) = universo(&[
        "guard",
        raiz.to_str().unwrap(),
        "--manifiesto",
        manifiesto.to_str().unwrap(),
        "--invariantes",
        invariantes.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(
        fs::read_to_string(raiz.join("notas/bienvenida.md"))
            .unwrap()
            .contains("Nota editada")
    );
    assert!(raiz.join("notas/fuente.md").exists());
    // Ya no hay diferencias: el delta queda vacío.
    let (_, vacio, _) = universo(&[
        "delta",
        raiz.to_str().unwrap(),
        "--desde",
        borrador.to_str().unwrap(),
        "--manifiesto",
        manifiesto.to_str().unwrap(),
    ]);
    assert_eq!(
        vacio.trim(),
        r#"{"schema":"felix-delta-v1","add":[],"remove":[]}"#
    );
}

#[test]
fn borrar_una_nota_en_el_borrador_es_error_y_no_escribe_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    assert_eq!(universo(&["init", repo.to_str().unwrap()]).0, 0);
    let borrador = tmp.path().join("borrador");
    copiar(&repo.join("bundle"), &borrador);
    fs::remove_file(borrador.join("notas/bienvenida.md")).unwrap();
    let (code, out, err) = universo(&[
        "delta",
        repo.join("bundle").to_str().unwrap(),
        "--desde",
        borrador.to_str().unwrap(),
        "--manifiesto",
        repo.join("manifest.toml").to_str().unwrap(),
    ]);
    assert_eq!(code, 1);
    assert_eq!(out, "");
    assert!(err.contains("note_deletion_unsupported"), "{err}");
}
