use std::path::PathBuf;
use universo_mcp::dominio::ServerConfig;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if matches!(args.as_slice(), [arg] if arg == "--version" || arg == "-V") {
        // Sin `println!`: con stdout cerrado no debe entrar en pánico.
        use std::io::Write;
        let texto = format!("universo-mcp {}\n", env!("CARGO_PKG_VERSION"));
        if std::io::stdout().write_all(texto.as_bytes()).is_err() {
            std::process::exit(1);
        }
        return;
    }
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
    let (mut criterio, mut universo, mut ignorar_version) = (None, None, false);
    while let Some(arg) = args.next() {
        if arg == "--ignorar-version" {
            if std::mem::replace(&mut ignorar_version, true) {
                return Err(());
            }
            continue;
        }
        let target = match arg.to_str() {
            Some("--root") => &mut root,
            Some("--manifiesto") => &mut manifiesto,
            Some("--invariantes") => &mut invariantes,
            Some("--criterio") => &mut criterio,
            Some("--universo") => &mut universo,
            _ => return Err(()),
        };
        if target.is_some() {
            return Err(());
        }
        *target = Some(PathBuf::from(args.next().ok_or(())?));
    }
    let (config, universos, por_defecto) = match criterio {
        // Repositorio de criterio (fase 10): las rutas salen de felix.toml.
        Some(dir) => {
            if root.is_some() || manifiesto.is_some() || invariantes.is_some() {
                return Err(());
            }
            // stdout es del protocolo: el motivo va a stderr.
            let informar = |e: universo_okf::criterio::ErrorCriterio| {
                eprintln!("criterio: error\ncode: {}\n{e}", e.codigo());
            };
            let repo = universo_okf::criterio::RepoCriterio::cargar(&dir).map_err(informar)?;
            if !ignorar_version {
                repo.comprobar_motor(env!("CARGO_PKG_VERSION"))
                    .map_err(informar)?;
            }
            let nombre = universo
                .as_ref()
                .map(|u| u.to_str().ok_or(()))
                .transpose()?;
            let elegido = repo.universo(nombre).map_err(informar)?;
            let config_de = |u: &universo_okf::criterio::Universo| ServerConfig {
                root: u.bundle(),
                manifiesto: u.manifiesto(),
                invariantes: u.invariantes(),
            };
            // Cada universo, para elegirlo por llamada (`universe`).
            let mut universos = Vec::new();
            for nombre in repo.universos.keys() {
                let u = repo.universo(Some(nombre)).map_err(informar)?;
                universos.push(universo_mcp::dominio::UniversoMcp {
                    nombre: u.nombre.clone(),
                    config: config_de(u),
                    escritura: u.escritura_agentes,
                });
            }
            (config_de(elegido), universos, Some(elegido.nombre.clone()))
        }
        None => {
            if universo.is_some() || ignorar_version {
                return Err(());
            }
            (
                ServerConfig {
                    root: root.ok_or(())?,
                    manifiesto: manifiesto.ok_or(())?,
                    invariantes: invariantes.ok_or(())?,
                },
                Vec::new(),
                None,
            )
        }
    };
    if worker {
        universo_mcp::ejecutar_worker(&config).map_err(|_| ())
    } else {
        universo_mcp::servir(config, universos, por_defecto).await
    }
}
