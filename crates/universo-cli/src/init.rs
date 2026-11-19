//! `universo init`: crea un repositorio de conocimiento mínimo, válido y sellado.
use super::{Formato, escribir_salida};
use serde_json::json;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use universo_okf::transaccion::sello::sellar;

pub(super) const USO: &str = "Uso: universo init <directorio> [--git] [--format text|json]\nCrea manifest.toml, invariants.sx y bundle/ en un directorio nuevo o vacío.\nCon --git crea un repositorio de criterio: felix.toml, los universos candidatos y vigente, README.md, AGENTS.md, .gitignore, .gitattributes y hooks de guard apagados (pre-commit y post-merge), y ejecuta git init.\n";

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

const GITIGNORE: &str = "# Transitorios de Felix: el lock y los archivos de una transacción a medias.\n# El historial (*.felix.log) y el sello (*.felix.seal) sí se versionan.\n*.felix.lock\n*.felix.journal\n*.felix.*.tmp\n*.felix.backup\n*.felix.stage.*\n*.felix.admctx*\n# Temporales de `universo reglas aplicar` y `manifiesto aplicar`.\n.felix-config-*\n*.felix-config.tmp\n";

const GITATTRIBUTES: &str = "# Finales de línea LF en todos los computadores: con CRLF el sello deja de\n# coincidir y `universo guard` falla.\n* text=auto eol=lf\n# El historial sólo crece: un merge conserva las líneas de ambos lados.\n*.felix.log merge=union\n";

const HOOK: &str = "#!/bin/sh\n# Hook de Felix: verifica cada universo antes de un commit. Se activa con\n#   git config core.hooksPath .githooks\n# FELIX_BIN elige otro binario de universo.\nset -e\nFELIX_BIN=\"${FELIX_BIN:-universo}\"\nraiz=\"$(git rev-parse --show-toplevel)\"\nfor u in candidatos vigente; do\n  \"$FELIX_BIN\" guard --criterio \"$raiz\" --universo \"$u\"\ndone\n";

/// Hook `post-merge`: tras un `git pull` o `merge`, corre `guard` en cada
/// universo y explica cómo aceptar un estado mezclado. Termina con 0: el merge ya
/// ocurrió.
const HOOK_POST_MERGE: &str = "#!/bin/sh\n# Hook de Felix: tras un merge, verifica cada universo y avisa. Se activa con\n#   git config core.hooksPath .githooks\n# FELIX_BIN elige otro binario de universo. Termina con 0 aunque guard falle: el merge ya ocurrió.\nFELIX_BIN=\"${FELIX_BIN:-universo}\"\nraiz=\"$(git rev-parse --show-toplevel)\" || exit 0\nif ! command -v \"$FELIX_BIN\" >/dev/null 2>&1; then\n  echo \"felix: no encuentro $FELIX_BIN; no se verificó el merge.\"\n  exit 0\nfi\nfor u in candidatos vigente; do\n  if ! \"$FELIX_BIN\" guard --criterio \"$raiz\" --universo \"$u\" >/dev/null 2>&1; then\n    echo \"felix: $u no coincide con su último estado admitido tras el merge.\"\n    echo \"felix:   revisa con: universo guard --universo $u && universo check --universo $u\"\n    echo \"felix:   si es correcto: universo seal --universo $u --motivo merge, y luego git commit\"\n  fi\ndone\nexit 0\n";

const README: &str = "# Repositorio de criterio\n\nCreado con `universo init --git` (Felix Felicis).\nGuarda criterio, no código: decisiones, principios y preferencias que Felix\nverifica antes de aceptar.\n\n## Universos\n\n- `universos/candidatos`: todo lo que se captura, con su contexto. Los agentes\n  pueden escribir aquí.\n- `universos/vigente`: criterio probado. Sólo entra por promoción.\n\n`felix.toml` los declara; `escritura_agentes` es por ahora una convención que\nla CLI no hace cumplir. Dentro de este directorio los comandos no necesitan\nrutas: `universo check`, `universo search <términos>`, `universo log`, y\n`--universo vigente` para el otro universo.\n\n## Forma y relaciones entre criterios\n\nUn criterio puede llevar `forma` (`principio`, `decision`, `preferencia` o\n`antipatron`): `universo atributo <nota> forma principio`. Viaja con la\npromoción. Entre criterios hay dos relaciones:\n\n- `reemplaza` (nuevo → anterior): en `vigente`, lo reemplazado queda\n  retirado. Retira primero el anterior\n  (`universo atributo <anterior> estado retirado --universo vigente`) y luego\n  `universo relacion <nuevo> reemplaza <anterior> --universo vigente`.\n- `contradice`: en `vigente` no puede haber dos criterios vigentes que se\n  contradigan.\n\nAl promover, estas relaciones viajan si el otro criterio ya está en el\ndestino, y las reglas de `vigente` deciden; si no está, `promover` dice que\nno las copió.\n\nUn criterio puede retirarse sin que otro lo reemplace.\n\n## Actualizar las plantillas\n\nUn repositorio creado con una versión anterior de Felix no conoce `forma`,\n`reemplaza` ni `contradice`. Crea uno nuevo en otro directorio con\n`universo init --git` y aplica sus archivos a cada universo, primero el\nmanifiesto y luego las reglas:\n\n```sh\nuniverso manifiesto aplicar <nuevo>/universos/<u>/manifest.toml --universo <u> --motivo plantillas\nuniverso reglas aplicar <nuevo>/universos/<u>/invariants.sx --universo <u> --motivo plantillas\n```\n\nSi tu criterio no cumple las reglas nuevas, `reglas aplicar` dice cuál falla\ny no cambia nada. Si ajustaste las reglas a mano, compáralas antes de aplicar.\n\n## Flujo entre computadores\n\n```sh\ngit pull --rebase\nuniverso guard --universo candidatos && universo guard --universo vigente\n# ... trabajar con universo ...\ngit add -A && git commit -m \"criterio: ...\"\ngit push\n```\n\nPara verificar antes de cada commit y avisar tras cada merge:\n`git config core.hooksPath .githooks` (`post-merge` corre con `git pull` o\n`git merge`, no con `git pull --rebase`; ahí corre `universo guard` a mano).\n\nSi dos computadores cambiaron el criterio a la vez, el merge une los\nhistoriales y el sello queda en conflicto: `universo guard` falla. Revisa el\nresultado (`universo check`), acéptalo con\n`universo seal --universo <u> --motivo merge` en cada universo afectado y haz\ncommit. `seal` deja la entrada en el historial y `universo diff` la nombra\n(`external_change`).\n\n## Privacidad\n\nQuien tenga acceso a este repositorio ve todo su criterio y su historial,\nincluso lo retirado. No guardes secretos ni credenciales.\n";

const AGENTS: &str = "# Instrucciones para agentes\n\nEste repositorio guarda el criterio de su dueño. Úsalo con la CLI `universo`\n(dentro de este directorio no hace falta pasar rutas) o con el MCP\n(`universo mcp-config` imprime la configuración): `search` con `universe: \"todos\"`,\n`context`, y `capture`, `reinforce` y `object` para guardar, que sólo escriben en\n`candidatos`.\n\n- **Antes de decidir**, consulta: `universo search <términos> --universo vigente`\n  y luego `--universo candidatos`.\n- **Cuando el dueño exprese criterio** («quiero que…», «siempre…», «prefiero…»),\n  guárdalo: `universo capturar \"<paráfrasis concisa>\" --proyecto <proyecto> --tarea <tarea>\n  --validacion explicita|implicita|pendiente --autor agente:<nombre>`. Guarda una\n  paráfrasis, no la conversación. Si responde `capture_similar`, ya existe: refuerza\n  la nota que indica en vez de duplicarla. La fecha por defecto es la UTC de hoy.\n- **Cuando un criterio se aplique en otro proyecto** y se sostenga:\n  `universo reforzar <nota> --proyecto <p> --contexto <c>`. Si el dueño lo\n  contradice: `universo objetar <nota> --proyecto <p> --contexto <c>`.\n- **Cambia el criterio sólo con Felix.** Para otros cambios, `universo propose\n  --delta <d.json>` simula y `--apply` publica. Nunca edites los archivos de\n  `universos/` a mano ni ejecutes `universo seal`.\n- **Escribe sólo en `candidatos`** y **no promuevas**: `universo promover` lo\n  ejecuta el dueño cuando lo decide. Es una regla de uso: la CLI no distingue\n  a una persona de un agente.\n- Para correcciones puntuales: `universo atributo <nota> <clave> <valor>`,\n  `universo relacion <origen> <clave> <destino> [--quitar]`.\n- Si Felix rechaza un cambio, lee la regla que falla y corrige; no la evites.\n- **Qué no es criterio:** órdenes puntuales de la tarea, datos del proyecto,\n  excepciones («por esta vez», «por ahora»), hipótesis y lo que dijo otro. Ante\n  la duda, pregunta en una línea antes de capturar.\n- **Al cerrar una sesión**, si el dueño lo acepta, lista las candidatas y guarda\n  sólo las que confirme. La guía completa es la skill `felix-criterio` del\n  repositorio de Felix (`skills/felix-criterio/SKILL.md`).\n";

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
    escribir(".githooks/post-merge", HOOK_POST_MERGE)?;
    #[cfg(unix)]
    for hook in [".githooks/pre-commit", ".githooks/post-merge"] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir.join(hook), fs::Permissions::from_mode(0o755))
            .map_err(|_| "init_io")?;
    }
    for universo in UNIVERSOS {
        let base = format!("universos/{universo}");
        escribir(
            &format!("{base}/manifest.toml"),
            universo_okf::registro_criterio::MANIFIESTO_CRITERIO,
        )?;
        let reglas = if universo == "vigente" {
            universo_okf::registro_criterio::INVARIANTES_VIGENTE
        } else {
            universo_okf::registro_criterio::INVARIANTES_CANDIDATOS
        };
        escribir(&format!("{base}/invariants.sx"), reglas)?;
        // La base empieza vacía. Git no versiona directorios vacíos: sin el
        // .gitkeep, un clon perdería los directorios y el universo no cargaría.
        for dir in ["criterios", "evidencias", "proyectos"] {
            escribir(&format!("{base}/bundle/{dir}/.gitkeep"), "")?;
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
