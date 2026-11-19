//! Fase 10, U1: `universo-mcp --criterio <dir> [--universo <n>]`.
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
};

const MANIFIESTO: &str = "version = \"criterio/1\"\n[nodos]\nsortes = [\"criterio\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"notas\"]\n";

/// Repositorio de criterio mínimo con dos universos; sólo `candidatos` tiene
/// una nota.
fn repo(dir: &Path, motor: &str) {
    std::fs::write(
        dir.join("felix.toml"),
        format!(
            "schema = \"felix-criterio-v1\"\nmotor = \"{motor}\"\npor_defecto = \"candidatos\"\n\
             [universos.candidatos]\ndir = \"u/candidatos\"\nescritura_agentes = true\n\
             [universos.vigente]\ndir = \"u/vigente\"\n"
        ),
    )
    .unwrap();
    for universo in ["candidatos", "vigente"] {
        let base = dir.join("u").join(universo);
        std::fs::create_dir_all(base.join("bundle/notas")).unwrap();
        std::fs::write(base.join("manifest.toml"), MANIFIESTO).unwrap();
        std::fs::write(base.join("invariants.sx"), "").unwrap();
    }
    std::fs::write(
        dir.join("u/candidatos/bundle/notas/reutilizar.md"),
        "---\ntype: criterio\ntitle: Reutilizar componentes\n---\nAntes de crear, reutilizar.\n",
    )
    .unwrap();
}

fn rango_actual() -> String {
    let mut partes = env!("CARGO_PKG_VERSION").split('.');
    let (mayor, menor) = (partes.next().unwrap(), partes.next().unwrap());
    let siguiente: u64 = menor.parse::<u64>().unwrap() + 1;
    format!(">={mayor}.{menor}, <{mayor}.{siguiente}")
}

/// Arranca el servidor, hace el handshake y llama a una herramienta.
fn llamar(args: &[&str], herramienta: &str, argumentos: Value) -> Value {
    let mut hijo = Command::new(env!("CARGO_BIN_EXE_universo-mcp"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = hijo.stdin.take().unwrap();
    let mut stdout = BufReader::new(hijo.stdout.take().unwrap());
    let mut enviar = |mensaje: Value| {
        writeln!(stdin, "{mensaje}").unwrap();
        stdin.flush().unwrap();
    };
    let mut linea = String::new();
    enviar(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"prueba","version":"1"}}}),
    );
    stdout.read_line(&mut linea).unwrap();
    enviar(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    enviar(
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":herramienta,"arguments":argumentos}}),
    );
    linea.clear();
    stdout.read_line(&mut linea).unwrap();
    drop(stdin);
    let _ = hijo.wait();
    serde_json::from_str::<Value>(&linea).unwrap()["result"]["structuredContent"].clone()
}

fn termina_con(args: &[&str]) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_universo-mcp"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap()
}

#[test]
fn con_criterio_resuelve_el_universo_por_defecto_o_el_pedido() {
    let dir = tempfile::tempdir().unwrap();
    repo(dir.path(), &rango_actual());
    let raiz = dir.path().to_str().unwrap();

    let check = llamar(
        &["--criterio", raiz],
        "check",
        json!({"schema":"felix-mcp-v1"}),
    );
    assert_eq!(check["status"], "admit", "{check}");

    let buscar = json!({"schema":"felix-mcp-v1","query":"reutilizar","include_values":true});
    let candidatos = llamar(&["--criterio", raiz], "search", buscar.clone());
    assert_eq!(candidatos["summary"]["total"], 1, "{candidatos}");
    let vigente = llamar(
        &["--criterio", raiz, "--universo", "vigente"],
        "search",
        buscar,
    );
    assert_eq!(vigente["summary"]["total"], 0, "{vigente}");
}

#[test]
fn con_criterio_rechaza_mezclar_rutas_universo_inexistente_y_version_fuera_de_rango() {
    let dir = tempfile::tempdir().unwrap();
    repo(dir.path(), &rango_actual());
    let raiz = dir.path().to_str().unwrap();
    assert_eq!(termina_con(&["--criterio", raiz, "--root", raiz]), 1);
    assert_eq!(
        termina_con(&["--criterio", raiz, "--universo", "axiomas"]),
        1
    );
    assert_eq!(termina_con(&["--universo", "vigente"]), 1);

    let otro = tempfile::tempdir().unwrap();
    repo(otro.path(), "=9.9.9");
    let otro = otro.path().to_str().unwrap();
    assert_eq!(termina_con(&["--criterio", otro]), 1);
    let check = llamar(
        &["--criterio", otro, "--ignorar-version"],
        "check",
        json!({"schema":"felix-mcp-v1"}),
    );
    assert_eq!(check["status"], "admit", "{check}");
}
