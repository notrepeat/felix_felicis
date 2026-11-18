use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

struct Client {
    child: Child,
    stdin: ChildStdin,
    replies: mpsc::Receiver<String>,
    id: u64,
}
impl Client {
    fn start(
        root: &std::path::Path,
        manifest: &std::path::Path,
        invariants: &std::path::Path,
    ) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_universo-mcp"))
            .args(["--root"])
            .arg(root)
            .arg("--manifiesto")
            .arg(manifest)
            .arg("--invariantes")
            .arg(invariants)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            stdin,
            replies: rx,
            id: 0,
        };
        let initialized = client.request("initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-agent","version":"1"}}));
        assert!(initialized.get("result").is_some(), "{initialized}");
        writeln!(
            client.stdin,
            "{}",
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .unwrap();
        client.stdin.flush().unwrap();
        client
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        writeln!(
            self.stdin,
            "{}",
            json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params})
        )
        .unwrap();
        self.stdin.flush().unwrap();
        loop {
            let line = self
                .replies
                .recv_timeout(Duration::from_secs(15))
                .expect("bounded MCP response");
            let reply: Value = serde_json::from_str(&line).unwrap();
            if reply["id"] == self.id {
                return reply;
            }
        }
    }
    fn tool(&mut self, name: &str, args: Value) -> Value {
        let reply = self.request("tools/call", json!({"name":name,"arguments":args}));
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn agent_context_reject_correct_readmit_restart_without_writes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    std::fs::create_dir_all(root.join("n")).unwrap();
    let bytes = "---\ntype: a\ntitle: PRIVATE_TITLE_CANARY\n---\nPRIVATE_BODY_CANARY\n";
    std::fs::write(root.join("n/item.md"), bytes).unwrap();
    let manifest = temp.path().join("manifest.toml");
    let invariants = temp.path().join("invariants.sx");
    std::fs::write(&manifest,"version = \"mcp/1\"\n[nodos]\nsortes = [\"a\", \"b\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n").unwrap();
    std::fs::write(&invariants, "; PRIVATE_COMMENT_CANARY\n(Type @n/item b)\n").unwrap();
    let mut client = Client::start(&root, &manifest, &invariants);
    let listed = client.request("tools/list", json!({}));
    assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 4);
    let redacted = client.tool("context", json!({"schema":"felix-mcp-v1","node":"n/item"}));
    for secret in [
        "PRIVATE_TITLE_CANARY",
        "PRIVATE_BODY_CANARY",
        "PRIVATE_COMMENT_CANARY",
    ] {
        assert!(!redacted.to_string().contains(secret));
    }
    let context = client.tool(
        "context",
        json!({"schema":"felix-mcp-v1","node":"n/item","include_values":true}),
    )["structuredContent"]
        .clone();
    assert!(!context.to_string().contains("PRIVATE_BODY_CANARY"));
    assert!(!context.to_string().contains("PRIVATE_COMMENT_CANARY"));
    // The synthetic agent obtains both target and current type from tool data.
    let formula = context["invariants"][0]["formula"].as_str().unwrap();
    let words: Vec<_> = formula
        .trim_matches(['(', ')'])
        .split_whitespace()
        .collect();
    assert_eq!(words[0], "Type");
    let desired = format!("Type({}, {})", words[1].trim_start_matches('@'), words[2]);
    let existing = context["atoms"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|a| a.starts_with("Type("))
        .unwrap()
        .to_owned();
    let rejected = json!({"schema":"felix-delta-v1","add":[desired],"remove":[]});
    let first = client.tool("propose", json!({"schema":"felix-mcp-v1","delta":rejected}));
    assert_ne!(first["isError"], true);
    assert_eq!(first["structuredContent"]["status"], "reject");
    let why = client.tool("why", json!({"schema":"felix-mcp-v1","delta":rejected}));
    assert_eq!(why["structuredContent"]["diagnostics"][0]["class"], "Type");
    let corrected = json!({"schema":"felix-delta-v1","add":[desired],"remove":[existing]});
    let accepted = client.tool(
        "propose",
        json!({"schema":"felix-mcp-v1","delta":corrected}),
    );
    assert_eq!(accepted["structuredContent"]["status"], "admit");
    let checked = client.tool("check", json!({"schema":"felix-mcp-v1","delta":corrected}));
    assert_eq!(checked["structuredContent"]["status"], "admit");
    let baseline = client.tool("check", json!({"schema":"felix-mcp-v1"}));
    assert_eq!(baseline["structuredContent"]["status"], "reject");
    assert_eq!(
        baseline["structuredContent"]["provenance"][0]["source_index"],
        0
    );
    for delta in [&rejected, &corrected] {
        let cli_engine = universo_okf::transaccion::simulacion::simular(
            &root,
            &manifest,
            &invariants,
            &delta.to_string(),
        )
        .unwrap();
        let mcp = client.tool("propose", json!({"schema":"felix-mcp-v1","delta":delta}));
        assert_eq!(
            mcp["structuredContent"]["summary"]["conflicts"],
            cli_engine.conflictos
        );
        assert_eq!(
            mcp["structuredContent"]["summary"]["failed_invariants"],
            cli_engine.invariantes_fallidas
        );
    }
    let invalid = client.tool(
        "propose",
        json!({"schema":"felix-mcp-v1","delta":corrected,"apply":true}),
    );
    assert_eq!(invalid["isError"], true);
    let unknown = client.request("tools/call", json!({"name":"apply","arguments":{}}));
    assert!(unknown.get("error").is_some());
    drop(client);
    let mut restarted = Client::start(&root, &manifest, &invariants);
    assert_eq!(
        restarted.tool(
            "propose",
            json!({"schema":"felix-mcp-v1","delta":corrected})
        ),
        accepted
    );
    assert_eq!(
        std::fs::read_to_string(root.join("n/item.md")).unwrap(),
        bytes
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 3);
}

#[test]
fn propose_acepta_delta_v2_que_crea_notas_sin_escribir() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    std::fs::create_dir_all(root.join("n")).unwrap();
    std::fs::write(root.join("n/item.md"), "---\ntype: a\ntitle: Item\n---\n").unwrap();
    let manifest = temp.path().join("manifest.toml");
    let invariants = temp.path().join("invariants.sx");
    std::fs::write(&manifest,"version = \"mcp/1\"\n[nodos]\nsortes = [\"a\", \"b\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n").unwrap();
    std::fs::write(&invariants, "(forall ((x N)) (Type x a))\n").unwrap();
    let mut client = Client::start(&root, &manifest, &invariants);
    let crear = |tipo: &str| json!({"schema":"felix-delta-v2","create":[{"path":"n/nueva","type":tipo,"title":"Nueva"}],"add":[],"remove":[]});
    let admit = client.tool(
        "propose",
        json!({"schema":"felix-mcp-v1","delta":crear("a")}),
    );
    assert_eq!(admit["structuredContent"]["status"], "admit", "{admit}");
    let reject = client.tool(
        "propose",
        json!({"schema":"felix-mcp-v1","delta":crear("b")}),
    );
    assert_eq!(reject["structuredContent"]["status"], "reject", "{reject}");
    assert!(!root.join("n/nueva.md").exists());
}

#[test]
fn el_rechazo_y_el_contexto_traen_id_y_mensaje_de_regla() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    std::fs::create_dir_all(root.join("n")).unwrap();
    std::fs::write(root.join("n/item.md"), "---\ntype: a\ntitle: X\n---\n").unwrap();
    let manifest = temp.path().join("manifest.toml");
    let invariants = temp.path().join("invariants.sx");
    std::fs::write(&manifest,"version = \"mcp/1\"\n[nodos]\nsortes = [\"a\", \"b\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n").unwrap();
    std::fs::write(
        &invariants,
        "(Type @n/item a)\n(regla :id \"tipo-b\" :mensaje \"El nodo es b.\" (Type @n/item b))\n",
    )
    .unwrap();
    let mut client = Client::start(&root, &manifest, &invariants);
    let checked =
        client.tool("check", json!({"schema":"felix-mcp-v1"}))["structuredContent"].clone();
    assert_eq!(checked["status"], "reject");
    assert_eq!(
        checked["provenance"],
        json!([{"source_index":1,"line":2,"column":1,"instance_index":null,"rule_id":"tipo-b","rule_message":"El nodo es b."}])
    );
    let context = client.tool("context", json!({"schema":"felix-mcp-v1","node":"n/item"}))["structuredContent"].clone();
    assert_eq!(context["invariants"][0]["rule_id"], Value::Null);
    assert_eq!(context["invariants"][1]["rule_id"], "tipo-b");
    assert_eq!(context["invariants"][1]["rule_message"], "El nodo es b.");
}
