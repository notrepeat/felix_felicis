//! CLI del motor Universo de Conocimiento: comandos de lectura.
use std::io::Write;
use std::path::{Path, PathBuf};

use universo_core::Manifiesto;
use universo_okf::{ErrorBundle, cargar_bundle, cargar_manifiesto};

/// Texto de uso, mostrado en `stderr` cuando los argumentos son inválidos.
const USO: &str = "Uso:\n  universo atomos <raiz> --manifiesto <manifiesto.toml>\n  universo verificar <raiz> --manifiesto <manifiesto.toml>\n";

/// Comando reconocido por la CLI, ya con sus argumentos resueltos.
enum Comando {
    Atomos { raiz: PathBuf, manifiesto: PathBuf },
    Verificar { raiz: PathBuf, manifiesto: PathBuf },
}

/// Interpreta los argumentos de línea de comandos (sin el nombre del
/// binario) como un [`Comando`]. Acepta `--manifiesto <valor>` y
/// `--manifiesto=<valor>`. Cualquier desvío del uso esperado devuelve el
/// texto de uso como error.
fn parsear_args(args: &[String]) -> Result<Comando, String> {
    let (nombre_comando, resto) = args.split_first().ok_or_else(|| USO.to_string())?;
    let es_atomos = match nombre_comando.as_str() {
        "atomos" => true,
        "verificar" => false,
        _ => return Err(USO.to_string()),
    };

    let mut raiz: Option<PathBuf> = None;
    let mut manifiesto: Option<PathBuf> = None;
    let mut i = 0;
    while i < resto.len() {
        let arg = &resto[i];
        if let Some(valor) = arg.strip_prefix("--manifiesto=") {
            if manifiesto.is_some() {
                return Err(USO.to_string());
            }
            manifiesto = Some(PathBuf::from(valor));
            i += 1;
        } else if arg == "--manifiesto" {
            let valor = resto.get(i + 1).ok_or_else(|| USO.to_string())?;
            if manifiesto.is_some() {
                return Err(USO.to_string());
            }
            manifiesto = Some(PathBuf::from(valor));
            i += 2;
        } else if raiz.is_none() {
            raiz = Some(PathBuf::from(arg));
            i += 1;
        } else {
            return Err(USO.to_string());
        }
    }

    let raiz = raiz.ok_or_else(|| USO.to_string())?;
    let manifiesto = manifiesto.ok_or_else(|| USO.to_string())?;
    Ok(if es_atomos {
        Comando::Atomos { raiz, manifiesto }
    } else {
        Comando::Verificar { raiz, manifiesto }
    })
}

/// Carga el manifiesto o escribe el error de carga en `error` y devuelve el
/// código de salida 1.
fn cargar_manifiesto_o_falla(ruta: &Path, error: &mut impl Write) -> Result<Manifiesto, i32> {
    cargar_manifiesto(ruta).map_err(|e| {
        let _ = writeln!(error, "{e}");
        1
    })
}

/// Escribe cada `ErrorBundle` de `errores` en `error`, uno por línea, y
/// devuelve el código de salida 1.
fn reportar_errores(errores: &[ErrorBundle], error: &mut impl Write) -> i32 {
    let mut buffer = String::new();
    for error_bundle in errores {
        buffer.push_str(&error_bundle.to_string());
        buffer.push('\n');
    }
    let _ = error.write_all(buffer.as_bytes());
    1
}

/// Ejecuta `atomos`: imprime cada átomo (orden canónico) en `salida`, uno
/// por línea; o los errores del bundle en `error`, uno por línea.
fn ejecutar_atomos(
    raiz: &Path,
    manifiesto: &Path,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    let manifiesto = match cargar_manifiesto_o_falla(manifiesto, error) {
        Ok(m) => m,
        Err(codigo) => return codigo,
    };
    match cargar_bundle(raiz, manifiesto) {
        Ok(estructura) => {
            let mut buffer = String::new();
            for atomo in estructura.atomos() {
                buffer.push_str(&atomo.to_string());
                buffer.push('\n');
            }
            let _ = salida.write_all(buffer.as_bytes());
            0
        }
        Err(errores) => reportar_errores(&errores, error),
    }
}

/// Ejecuta `verificar`: imprime `OK: N nodos, M aristas` en `salida` si el
/// bundle carga sin errores; o los errores del bundle en `error`, uno por
/// línea.
fn ejecutar_verificar(
    raiz: &Path,
    manifiesto: &Path,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    let manifiesto = match cargar_manifiesto_o_falla(manifiesto, error) {
        Ok(m) => m,
        Err(codigo) => return codigo,
    };
    match cargar_bundle(raiz, manifiesto) {
        Ok(estructura) => {
            let mensaje = format!(
                "OK: {} nodos, {} aristas\n",
                estructura.len_nodos(),
                estructura.len_aristas()
            );
            let _ = salida.write_all(mensaje.as_bytes());
            0
        }
        Err(errores) => reportar_errores(&errores, error),
    }
}

/// Interpreta `args` y ejecuta el comando correspondiente, escribiendo en
/// `salida`/`error` y devolviendo el código de salida del proceso. Nunca
/// entra en pánico ante entradas de usuario inválidas.
pub(crate) fn ejecutar(args: Vec<String>, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let comando = match parsear_args(&args) {
        Ok(comando) => comando,
        Err(uso) => {
            let _ = error.write_all(uso.as_bytes());
            return 2;
        }
    };
    match comando {
        Comando::Atomos { raiz, manifiesto } => ejecutar_atomos(&raiz, &manifiesto, salida, error),
        Comando::Verificar { raiz, manifiesto } => {
            ejecutar_verificar(&raiz, &manifiesto, salida, error)
        }
    }
}

fn main() {
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();

    let mut args: Vec<String> = Vec::new();
    for arg in std::env::args_os().skip(1) {
        match arg.into_string() {
            Ok(s) => args.push(s),
            Err(_) => {
                let _ = stderr.write_all(USO.as_bytes());
                std::process::exit(2);
            }
        }
    }

    std::process::exit(ejecutar(args, &mut stdout, &mut stderr));
}
