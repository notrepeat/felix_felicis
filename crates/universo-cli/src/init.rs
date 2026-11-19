//! `universo init`: crea un repositorio de conocimiento mínimo, válido y sellado.
use super::{Formato, escribir_salida};
use serde_json::json;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use universo_okf::transaccion::sello::sellar;

pub(super) const USO: &str = "Uso: universo init <directorio> [--git] [--format text|json]\nCrea manifest.toml, invariants.sx y bundle/ en un directorio nuevo o vacío.\nCon --git crea un repositorio de criterio: felix.toml, los universos candidatos y vigente, README.md, AGENTS.md, .gitignore, .gitattributes y un hook de guard apagado, y ejecuta git init.\n";

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
    git: bool,
}

pub(super) fn parsear(args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let (mut directorio, mut formato, mut git) = (None, None, false);
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
        if arg == "--git" {
            if std::mem::replace(&mut git, true) {
                return Err(fail());
            }
            i += 1;
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
        git,
    })
}

/// Exige `dir` vacío o inexistente, y lo crea.
fn preparar_directorio(dir: &Path) -> Result<(), &'static str> {
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
    Ok(())
}

/// Crea `relativa` dentro de `dir` sin sobrescribir nada.
fn escribir_en(dir: &Path, relativa: &str, contenido: &str) -> io::Result<()> {
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
}

fn crear(dir: &Path) -> Result<(), &'static str> {
    preparar_directorio(dir)?;
    let escribir = |relativa: &str, contenido: &str| escribir_en(dir, relativa, contenido);
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

const UNIVERSOS: [&str; 2] = ["candidatos", "vigente"];

const GITIGNORE: &str = "# Transitorios de Felix: el lock y los archivos de una transacción a medias.\n# El historial (*.felix.log) y el sello (*.felix.seal) sí se versionan.\n*.felix.lock\n*.felix.journal\n*.felix.*.tmp\n*.felix.backup\n*.felix.stage.*\n*.felix.admctx*\n";

const GITATTRIBUTES: &str = "# Finales de línea LF en todos los computadores: con CRLF el sello deja de\n# coincidir y `universo guard` falla.\n* text=auto eol=lf\n# El historial sólo crece: un merge conserva las líneas de ambos lados.\n*.felix.log merge=union\n";

const HOOK: &str = "#!/bin/sh\n# Hook de Felix: verifica cada universo antes de un commit. Se activa con\n#   git config core.hooksPath .githooks\n# FELIX_BIN elige otro binario de universo.\nset -e\nFELIX_BIN=\"${FELIX_BIN:-universo}\"\nraiz=\"$(git rev-parse --show-toplevel)\"\nfor u in candidatos vigente; do\n  \"$FELIX_BIN\" guard --criterio \"$raiz\" --universo \"$u\"\ndone\n";

const README: &str = "# Repositorio de criterio\n\nCreado con `universo init --git` (Felix Felicis).\nGuarda criterio, no código: decisiones, principios y preferencias que Felix\nverifica antes de aceptar.\n\n## Universos\n\n- `universos/candidatos`: todo lo que se captura, con su contexto. Los agentes\n  pueden escribir aquí.\n- `universos/vigente`: criterio probado. Sólo entra por promoción.\n\n`felix.toml` los declara; `escritura_agentes` es por ahora una convención que\nla CLI no hace cumplir. Dentro de este directorio los comandos no necesitan\nrutas: `universo check`, `universo search <términos>`, `universo log`, y\n`--universo vigente` para el otro universo.\n\n## Flujo entre computadores\n\n```sh\ngit pull --rebase\nuniverso guard --universo candidatos && universo guard --universo vigente\n# ... trabajar con universo ...\ngit add -A && git commit -m \"criterio: ...\"\ngit push\n```\n\nPara verificar antes de cada commit: `git config core.hooksPath .githooks`.\n\n## Privacidad\n\nQuien tenga acceso a este repositorio ve todo su criterio y su historial,\nincluso lo retirado. No guardes secretos ni credenciales.\n";

const AGENTS: &str = "# Instrucciones para agentes\n\nEste repositorio guarda el criterio de su dueño. Úsalo con la CLI `universo`\n(o con `universo-mcp`); dentro de este directorio no hace falta pasar rutas.\n\n- **Antes de decidir**, consulta: `universo search <términos> --universo vigente`\n  y luego `--universo candidatos`.\n- **Cambia el criterio sólo con Felix**: `universo propose --delta <d.json>` simula, y\n  `--apply` publica con el token de la simulación. Nunca edites los archivos de\n  `universos/` a mano ni ejecutes `universo seal`.\n- **Escribe sólo en `candidatos`**. A `vigente` se llega por promoción, que\n  confirma el dueño. Es una regla de uso: la CLI todavía no lo impide.\n- Si Felix rechaza un cambio, lee la regla que falla y corrige; no la evites.\n";

const FELIX_TOML_PLANTILLA: &str = "# Repositorio de criterio creado por `universo init --git`.\nschema = \"felix-criterio-v1\"\nmotor = \"{motor}\"\npor_defecto = \"candidatos\"\n\n# Todo lo que se captura, con su contexto. Los agentes pueden escribir aquí.\n[universos.candidatos]\ndir = \"universos/candidatos\"\nescritura_agentes = true\n\n# Criterio probado: sólo entra por promoción.\n[universos.vigente]\ndir = \"universos/vigente\"\nescritura_agentes = false\n";

/// Repositorio de criterio: `felix.toml`, dos universos sellados, archivos de
/// Git y `git init`. No hace commits ni configura remotos.
fn crear_repositorio(dir: &Path) -> Result<(), &'static str> {
    // Sin git no se escribe nada: no quedan archivos a medias.
    let disponible = std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|s| s.status.success());
    if !disponible {
        return Err("init_git_unavailable");
    }
    preparar_directorio(dir)?;
    let escribir = |relativa: &str, contenido: &str| {
        escribir_en(dir, relativa, contenido).map_err(|_| "init_io")
    };
    let motor = universo_okf::criterio::rango_para(env!("CARGO_PKG_VERSION"));
    escribir(
        universo_okf::criterio::ARCHIVO,
        &FELIX_TOML_PLANTILLA.replace("{motor}", &motor),
    )?;
    escribir("README.md", README)?;
    escribir("AGENTS.md", AGENTS)?;
    escribir(".gitignore", GITIGNORE)?;
    escribir(".gitattributes", GITATTRIBUTES)?;
    escribir(".githooks/pre-commit", HOOK)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            dir.join(".githooks/pre-commit"),
            fs::Permissions::from_mode(0o755),
        )
        .map_err(|_| "init_io")?;
    }
    for universo in UNIVERSOS {
        let base = format!("universos/{universo}");
        escribir(&format!("{base}/manifest.toml"), MANIFIESTO)?;
        escribir(&format!("{base}/invariants.sx"), INVARIANTES)?;
        if universo == "candidatos" {
            escribir(&format!("{base}/bundle/notas/bienvenida.md"), NOTA)?;
        } else {
            // Git no versiona directorios vacíos: sin esto, un clon perdería
            // el directorio de notas y el universo no cargaría.
            escribir(&format!("{base}/bundle/notas/.gitkeep"), "")?;
        }
        let raiz = dir.join(&base);
        sellar(
            &raiz.join("bundle"),
            &raiz.join("manifest.toml"),
            &raiz.join("invariants.sx"),
        )
        .map_err(|e| e.codigo())?;
    }
    let git = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .output()
        .map_err(|_| "init_git_unavailable")?;
    if !git.status.success() {
        return Err("init_git_failed");
    }
    Ok(())
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    if s.git {
        return ejecutar_repositorio(&s, salida, error);
    }
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

fn ejecutar_repositorio(s: &Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let (payload, codigo) = match (crear_repositorio(&s.directorio), s.formato) {
        (Ok(()), Formato::Json) => (
            format!(
                "{}\n",
                json!({"schema_version":1,"command":"init","status":"ok","repository":"criterio",
                       "universes":UNIVERSOS,"default_universe":"candidatos","sealed":true,"git":true,"errors":[]})
            ),
            0,
        ),
        (Ok(()), Formato::Texto) => (
            format!(
                "init: ok\nrepository: criterio\nuniverses: {} (default: candidatos)\nsealed: yes\ngit: initialized, no commits\nnext: cd {} && universo guard\n",
                UNIVERSOS.join(", "),
                s.directorio.display()
            ),
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
