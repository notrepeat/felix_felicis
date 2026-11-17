use std::path::PathBuf;
use universo_mcp::dominio::ServerConfig;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if run().await.is_err() {
        eprintln!("mcp_server_failed");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), ()> {
    let mut args = std::env::args_os().skip(1).peekable();
    let worker = args.peek().is_some_and(|arg| arg == "--worker");
    if worker {
        args.next();
    }
    let (mut root, mut manifiesto, mut invariantes) = (None, None, None);
    while let Some(arg) = args.next() {
        let target = match arg.to_str() {
            Some("--root") => &mut root,
            Some("--manifiesto") => &mut manifiesto,
            Some("--invariantes") => &mut invariantes,
            _ => return Err(()),
        };
        if target.is_some() {
            return Err(());
        }
        *target = Some(PathBuf::from(args.next().ok_or(())?));
    }
    let config = ServerConfig {
        root: root.ok_or(())?,
        manifiesto: manifiesto.ok_or(())?,
        invariantes: invariantes.ok_or(())?,
    };
    if worker {
        universo_mcp::ejecutar_worker(&config).map_err(|_| ())
    } else {
        universo_mcp::servir(config).await
    }
}
