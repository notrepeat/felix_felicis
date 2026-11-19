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

/// Repositorio con el esquema de captura de `init --git`: candidatos admite
/// escritura de agentes, vigente no.
fn repo_captura(dir: &Path) {
    use universo_okf::registro_criterio::{
        INVARIANTES_CANDIDATOS, INVARIANTES_VIGENTE, MANIFIESTO_CRITERIO,
    };
    std::fs::write(
        dir.join("felix.toml"),
        format!(
            "schema = \"felix-criterio-v1\"\nmotor = \"{}\"\npor_defecto = \"candidatos\"\n\
             [universos.candidatos]\ndir = \"u/candidatos\"\nescritura_agentes = true\n\
             [universos.vigente]\ndir = \"u/vigente\"\n",
            rango_actual()
        ),
    )
    .unwrap();
    for (universo, reglas) in [
        ("candidatos", INVARIANTES_CANDIDATOS),
        ("vigente", INVARIANTES_VIGENTE),
    ] {
        let base = dir.join("u").join(universo);
        for sub in ["criterios", "evidencias", "proyectos"] {
            std::fs::create_dir_all(base.join("bundle").join(sub)).unwrap();
        }
        std::fs::write(base.join("manifest.toml"), MANIFIESTO_CRITERIO).unwrap();
        std::fs::write(base.join("invariants.sx"), reglas).unwrap();
    }
}

/// Sesión MCP: handshake y una lista de peticiones; devuelve cada `result`.
fn sesion(args: &[&str], peticiones: &[(&str, Value)]) -> Vec<Value> {
    let mut hijo = Command::new(env!("CARGO_BIN_EXE_universo-mcp"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = hijo.stdin.take().unwrap();
    let mut stdout = BufReader::new(hijo.stdout.take().unwrap());
    let mut linea = String::new();
    writeln!(stdin, "{}", json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"prueba","version":"1"}}})).unwrap();
    stdout.read_line(&mut linea).unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    let mut resultados = Vec::new();
    for (i, (metodo, params)) in peticiones.iter().enumerate() {
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc":"2.0","id":i + 1,"method":metodo,"params":params})
        )
        .unwrap();
        stdin.flush().unwrap();
        linea.clear();
        stdout.read_line(&mut linea).unwrap();
        let respuesta: Value = serde_json::from_str(&linea).unwrap();
        resultados.push(respuesta["result"].clone());
    }
    drop(stdin);
    let _ = hijo.wait();
    resultados
}

fn herramienta(nombre: &str, argumentos: Value) -> (&'static str, Value) {
    (
        "tools/call",
        json!({"name": nombre, "arguments": argumentos}),
    )
}

#[test]
fn con_criterio_las_herramientas_aceptan_universo_y_anuncian_la_captura() {
    let dir = tempfile::tempdir().unwrap();
    repo_captura(dir.path());
    let raiz = dir.path().to_str().unwrap();
    let lista = &sesion(&["--criterio", raiz], &[("tools/list", json!({}))])[0]["tools"];
    let nombres: Vec<&str> = lista
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for esperado in [
        "context",
        "propose",
        "check",
        "why",
        "search",
        "capture",
        "reinforce",
        "object",
    ] {
        assert!(nombres.contains(&esperado), "{nombres:?}");
    }
    for t in lista.as_array().unwrap() {
        assert!(
            t["inputSchema"]["properties"]["universe"].is_object(),
            "{t}"
        );
    }
    // Sin --criterio: ni captura ni universo.
    let otro = tempfile::tempdir().unwrap();
    repo(otro.path(), &rango_actual());
    let u = otro.path().join("u/candidatos");
    let (b, m, i) = (
        u.join("bundle"),
        u.join("manifest.toml"),
        u.join("invariants.sx"),
    );
    let lista = &sesion(
        &[
            "--root",
            b.to_str().unwrap(),
            "--manifiesto",
            m.to_str().unwrap(),
            "--invariantes",
            i.to_str().unwrap(),
        ],
        &[("tools/list", json!({}))],
    )[0]["tools"];
    assert_eq!(lista.as_array().unwrap().len(), 5, "{lista}");
    assert!(lista[0]["inputSchema"]["properties"]["universe"].is_null());
}

#[test]
fn un_agente_captura_y_refuerza_en_candidatos_pero_no_en_vigente() {
    let dir = tempfile::tempdir().unwrap();
    repo_captura(dir.path());
    let raiz = dir.path().to_str().unwrap();
    let r = sesion(
        &["--criterio", raiz],
        &[
            herramienta(
                "capture",
                json!({"schema":"felix-mcp-v1","text":"Reutilizar antes de crear",
                "project":"Tienda web","task":"carrito","validation":"explicita","author":"prueba"}),
            ),
            herramienta(
                "reinforce",
                json!({"schema":"felix-mcp-v1","note":"criterios/reutilizar-antes-de-crear",
                "project":"API pagos","context":"módulo de cobros","author":"prueba"}),
            ),
            herramienta(
                "capture",
                json!({"schema":"felix-mcp-v1","universe":"vigente","text":"Otra",
                "project":"P","author":"prueba"}),
            ),
            herramienta(
                "search",
                json!({"schema":"felix-mcp-v1","universe":"todos","query":"reutilizar","include_values":true}),
            ),
            herramienta(
                "check",
                json!({"schema":"felix-mcp-v1","universe":"vigente"}),
            ),
            herramienta(
                "check",
                json!({"schema":"felix-mcp-v1","universe":"axiomas"}),
            ),
            herramienta(
                "capture",
                json!({"schema":"felix-mcp-v1","text":"Sin autor","project":"P"}),
            ),
        ],
    );
    let captura = &r[0]["structuredContent"];
    assert_eq!(captura["status"], "applied", "{}", r[0]);
    assert_eq!(captura["note"], "criterios/reutilizar-antes-de-crear");
    assert_eq!(captura["universe"], "candidatos");
    assert_eq!(r[1]["structuredContent"]["status"], "applied", "{}", r[1]);
    assert!(
        r[1]["structuredContent"]["evidence"]
            .as_str()
            .unwrap()
            .starts_with("evidencias/")
    );
    assert_eq!(r[2]["isError"], true, "{}", r[2]);
    assert_eq!(r[2]["structuredContent"]["code"], "universe_read_only");
    let matches = r[3]["structuredContent"]["matches"].as_array().unwrap();
    assert!(
        matches.iter().any(|m| m["universe"] == "candidatos"),
        "{}",
        r[3]
    );
    assert_eq!(r[4]["structuredContent"]["status"], "admit", "{}", r[4]);
    assert_eq!(r[5]["isError"], true);
    assert_eq!(r[5]["structuredContent"]["code"], "universe_not_found");
    assert_eq!(r[6]["isError"], true, "{}", r[6]);
    // El autor queda como agente en el historial.
    let log = std::fs::read_to_string(dir.path().join("u/candidatos/.bundle.felix.log")).unwrap();
    assert!(log.contains("\"author\":\"agente:prueba\""), "{log}");
}

#[test]
fn universe_en_un_servidor_sin_criterio_es_un_error() {
    let dir = tempfile::tempdir().unwrap();
    repo(dir.path(), &rango_actual());
    let u = dir.path().join("u/candidatos");
    let (b, m, i) = (
        u.join("bundle"),
        u.join("manifest.toml"),
        u.join("invariants.sx"),
    );
    let r = sesion(
        &[
            "--root",
            b.to_str().unwrap(),
            "--manifiesto",
            m.to_str().unwrap(),
            "--invariantes",
            i.to_str().unwrap(),
        ],
        &[herramienta(
            "check",
            json!({"schema":"felix-mcp-v1","universe":"vigente"}),
        )],
    );
    assert_eq!(r[0]["isError"], true, "{}", r[0]);
}

/// U12: una objeción aparece en `context` del criterio (con valores).
#[test]
fn una_objecion_aparece_en_el_contexto_del_criterio() {
    let dir = tempfile::tempdir().unwrap();
    repo_captura(dir.path());
    let raiz = dir.path().to_str().unwrap();
    let r = sesion(
        &["--criterio", raiz],
        &[
            herramienta(
                "capture",
                json!({"schema":"felix-mcp-v1","text":"Reutilizar antes de crear",
                "project":"Tienda web","author":"prueba"}),
            ),
            herramienta(
                "object",
                json!({"schema":"felix-mcp-v1","note":"criterios/reutilizar-antes-de-crear",
                "project":"Prototipo","context":"se prefirió duplicar","author":"prueba"}),
            ),
            herramienta(
                "context",
                json!({"schema":"felix-mcp-v1","node":"criterios/reutilizar-antes-de-crear",
                "depth":1,"include_values":true}),
            ),
        ],
    );
    assert_eq!(r[1]["structuredContent"]["status"], "applied", "{}", r[1]);
    let evidencia = r[1]["structuredContent"]["evidence"].as_str().unwrap();
    let atomos = r[2]["structuredContent"]["atoms"].as_array().unwrap();
    assert!(
        atomos.iter().any(|a| {
            let a = a.as_str().unwrap();
            a.starts_with("Edge(")
                && a.contains(evidencia)
                && a.ends_with(", criterios/reutilizar-antes-de-crear, objeta)")
        }),
        "{atomos:?}"
    );
}
