//! `universo init`: crea un repositorio de conocimiento mínimo, válido y sellado.
use super::{Formato, escribir_salida};
use serde_json::json;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use universo_okf::transaccion::sello::sellar;

pub(super) const USO: &str = "Uso: universo init <directorio> [--format text|json]\nCrea manifest.toml, invariants.sx y bundle/ en un directorio nuevo o vacío.\n";

const MANIFIESTO: &str = r#"# Manifiesto inicial creado por `universo init`. Ajusta sortes, aristas y
# predicados a tu criterio antes de agregar conocimiento.
version = "criterio/1"

[nodos]
sortes = ["criterio", "fuente", "Retirado", "Patrón"]

[[aristas]]
clave = "apoya"

[[predicados]]
nombre = "Estado"
clave = "estado"
dominio = ["borrador", "vigente"]

[layout]
directorios = ["notas"]
"#;

const INVARIANTES: &str = r#"; Invariantes iniciales creadas por `universo init`.
(regla :id "tipo-conocido" :mensaje "Cada nota es un criterio o una fuente."
  (forall ((x N))
    (or (Type x criterio) (Type x fuente))))

(regla :id "vigente-es-criterio" :mensaje "Sólo un criterio puede estar vigente."
  (forall ((x N))
    (=> (Estado x vigente) (Type x criterio))))

(regla :id "apoyo-de-fuente" :mensaje "Una arista apoya va de una fuente a un criterio."
  (forall ((e E) (x N) (y N))
    :guarda (Edge e x y apoya)
    (and (Type x fuente) (Type y criterio))))
"#;

const NOTA: &str = "---\ntype: criterio\ntitle: Bienvenida\nestado: borrador\n---\n\nPrimera nota del repositorio. Los cambios entran con `universo propose`\ny `universo guard` detecta los que se hagan fuera de Felix.\n";

pub(super) struct Solicitud {
    directorio: PathBuf,
    formato: Formato,
}

pub(super) fn parsear(args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let (mut directorio, mut formato) = (None, None);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let valor = if let Some(v) = arg.strip_prefix("--format=") {
            i += 1;
            Some(v)
        } else if arg == "--format" {
            i += 2;
            Some(args.get(i - 1).ok_or_else(fail)?.as_str())
        } else {
            None
        };
        if let Some(valor) = valor {
            let nuevo = match valor {
                "text" => Formato::Texto,
                "json" => Formato::Json,
                _ => return Err(fail()),
            };
            if formato.replace(nuevo).is_some() {
                return Err(fail());
            }
            continue;
        }
        if arg.is_empty()
            || arg.starts_with('-')
            || directorio.replace(PathBuf::from(arg)).is_some()
        {
            return Err(fail());
        }
        i += 1;
    }
    Ok(Solicitud {
        directorio: directorio.ok_or_else(fail)?,
        formato: formato.unwrap_or(Formato::Texto),
    })
}

fn crear(dir: &Path) -> Result<(), &'static str> {
    match fs::read_dir(dir) {
        Ok(mut entradas) => {
            if entradas.next().is_some() {
                return Err("init_target_not_empty");
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(dir).map_err(|_| "init_io")?
        }
        Err(_) => return Err("init_target_invalid"),
    }
    let escribir = |relativa: &str, contenido: &str| -> io::Result<()> {
        let ruta = dir.join(relativa);
        if let Some(padre) = ruta.parent() {
            fs::create_dir_all(padre)?;
        }
        let mut archivo = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(ruta)?;
        archivo.write_all(contenido.as_bytes())?;
        archivo.sync_all()
    };
    escribir("manifest.toml", MANIFIESTO).map_err(|_| "init_io")?;
    escribir("invariants.sx", INVARIANTES).map_err(|_| "init_io")?;
    escribir("bundle/notas/bienvenida.md", NOTA).map_err(|_| "init_io")?;
    sellar(
        &dir.join("bundle"),
        &dir.join("manifest.toml"),
        &dir.join("invariants.sx"),
    )
    .map_err(|e| e.codigo())
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let (payload, codigo) = match (crear(&s.directorio), s.formato) {
        (Ok(()), Formato::Json) => (
            format!(
                "{}\n",
                json!({"schema_version":1,"command":"init","status":"ok","created":["manifest.toml","invariants.sx","bundle/notas/bienvenida.md"],"sealed":true,"errors":[]})
            ),
            0,
        ),
        (Ok(()), Formato::Texto) => (
            "init: ok\ncreated: manifest.toml, invariants.sx, bundle/notas/bienvenida.md\nsealed: yes\nnext: universo guard <directorio>/bundle --manifiesto <directorio>/manifest.toml --invariantes <directorio>/invariants.sx\n".to_string(),
            0,
        ),
        (Err(code), Formato::Json) => (
            format!(
                "{}\n",
                json!({"schema_version":1,"command":"init","status":"error","errors":[{"code":code}]})
            ),
            1,
        ),
        (Err(code), Formato::Texto) => (format!("init: error\ncode: {code}\n"), 1),
    };
    if !escribir_salida(salida, error, payload.as_bytes(), "stdout") {
        return 1;
    }
    codigo
}
