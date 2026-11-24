use std::path::PathBuf;
use universo_mcp::dominio::ServerConfig;

const AYUDA: &str = "Servidor MCP local de Felix Felicis (stdio)

Uso:
  universo-mcp --root <bundle> --manifiesto <manifest.toml> --invariantes <invariants.sx>
  universo-mcp --criterio <dir> [--universo <nombre>] [--ignorar-version] [--registrar-uso]

Opciones:
  --root <dir>           Bundle que sirve el servidor (con --manifiesto e --invariantes).
  --manifiesto <ruta>    Manifiesto TOML del bundle.
  --invariantes <ruta>   Programa de invariantes del bundle.
  --criterio <dir>       Repositorio de criterio (felix.toml): las rutas salen de ahí. No se mezcla con --root, --manifiesto ni --invariantes.
  --universo <nombre>    Con --criterio: universo por defecto de las herramientas (si no, el de felix.toml).
  --ignorar-version      Con --criterio: arranca aunque la versión no cumpla el rango motor de felix.toml.
  --registrar-uso        Con --criterio: anota cada llamada, sin contenido, en .git/felix-uso-mcp.jsonl para universo metricas.
  -h, --help             Muestra esta ayuda y termina con código 0.
  -V, --version          Muestra la versión y termina con código 0.

stdout es del protocolo MCP; los errores de arranque van a stderr.
Ver docs/mcp.md.
";

/// Escribe en stdout sin `println!`: con stdout cerrado no debe entrar en pánico.
fn imprimir(texto: &str) {
    use std::io::Write;
    if std::io::stdout().write_all(texto.as_bytes()).is_err() {
        std::process::exit(1);
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if matches!(args.as_slice(), [arg] if arg == "--version" || arg == "-V") {
        imprimir(&format!("universo-mcp {}\n", env!("CARGO_PKG_VERSION")));
        return;
    }
    if matches!(args.as_slice(), [arg] if arg == "--help" || arg == "-h") {
        imprimir(AYUDA);
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
    let mut registrar_uso = false;
    while let Some(arg) = args.next() {
        if arg == "--registrar-uso" {
            if std::mem::replace(&mut registrar_uso, true) {
                return Err(());
            }
            continue;
        }
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
            // Uso local para `universo metricas`: dentro de .git, nunca versionado.
            universo_mcp::usar_repositorio(repo.raiz.clone());
            if registrar_uso {
                if let Some(git) = universo_okf::criterio::dir_git(&repo.raiz) {
                    universo_mcp::registrar_uso_en(git.join("felix-uso-mcp.jsonl"));
                }
            }
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
            if universo.is_some() || ignorar_version || registrar_uso {
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
