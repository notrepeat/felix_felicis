//! CLI del motor Universo de Conocimiento: lectura y aplicación transaccional.
mod apply;
mod busqueda;
mod captura_criterio;
mod conectar;
mod configuracion;
mod criterio;
mod diferencia;
mod diff;
mod edicion;
mod estado;
mod grafo;
mod guardia;
mod historial;
mod init;
mod metricas;
mod promocion;
mod propose;
mod resumen;
mod testigos;

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use universo_core::invariante::expansion::{
    ErrorExpansion, expandir_sentencias, expandir_sentencias_fuente,
};
use universo_core::invariante::validar_sentencia_fuente;
use universo_core::sexpr::{ErrorAutoria, SentenciaFuente, parsear_programa};
use universo_core::veredicto::{
    ErrorVeredicto, ModoExplicacion, explicar_con_presupuesto_y_consumos,
};
use universo_core::{ErrorEvaluacion, Manifiesto, PRESUPUESTO_EXPLICACION_POR_DEFECTO, evaluar};
use universo_okf::explicacion_json::{
    anotar_reglas, error_explicacion_codigo, explicaciones_medidas_a_json,
};
use universo_okf::{ErrorBundle, ErrorManifiestoArchivo, Regla, cargar_bundle, cargar_manifiesto};

/// Uso breve, mostrado por errores de argumentos en `stderr`.
const USO: &str = "Uso:\n  universo atomos <raiz> --manifiesto <manifiesto.toml>\n  universo validate <raiz> --manifiesto <manifiesto.toml> [--format <text|json>]\n  universo verificar <raiz> --manifiesto <manifiesto.toml> [--format <text|json>]  (alias de validate)\n  universo check <raiz> --manifiesto <manifiesto.toml> --invariantes <archivo> [--explain <decide|enumerate> [--rules] | --witness [--witness-limit <n>]] [--format <text|json>]\n  universo propose <raiz> --manifiesto <m> --invariantes <i> --delta <d.json> [--format text|json] [--explain decide|enumerate [--rules]] [--witness [all] [--witness-limit <n>]] [--emit-snapshot-token | --apply --expect-snapshot <token> [--author <a>] [--reason <r>]]\n  universo status <raiz> [--format <text|json>]\n  universo recover <raiz> [--format <text|json>]\n  universo guard <raiz> --manifiesto <m> --invariantes <i> [--format <text|json>]\n  universo seal <raiz> --manifiesto <m> --invariantes <i> [--autor <a>] [--motivo <m>] [--format <text|json>]\n  universo init <directorio> [--git] [--format <text|json>]\n  universo log <raiz> [--format <text|json>]\n  universo show <raiz> <n> [--format <text|json>]\n  universo diff <raiz> <desde> [<hasta>] [--format <text|json>]\n  universo delta <raiz> --desde <borrador> --manifiesto <m>\n  universo search <raiz> --manifiesto <m> <términos...> [--limit <n>] [--format <text|json>]\n  universo capturar <texto> --proyecto <nombre> [--tarea <t>] [--validacion <v>] [--dry-run]\n  universo reforzar|objetar <nota> --proyecto <nombre> --contexto <c> [--validacion <v>] [--dry-run]\n  universo promover <nota> --a <universo> [--de <universo>] [--dry-run]\n  universo mcp-config [--cliente claude-code|codex|json] [--nombre <servidor>]\n  universo conectar [--aplicar | --verificar] [--format text|json]\n  universo grafo [--universo <nombre>] [--salida <archivo.html>|-]\n  universo metricas [--hasta AAAA-MM-DD] [--estancada-dias <n>] [--format text|json]\n  universo resumen [--criterio <dir>] [--format <text|json>]\n  universo relacion <origen> <clave> <destino> [--quitar] | atributo <nota> <clave> <valor> [--quitar] | nota editar <nota>\n  universo reglas aplicar <archivo.sx> | manifiesto aplicar <archivo.toml> [--autor <a>] [--motivo <m>] [--dry-run]\nUse --manifiesto=<ruta> y --invariantes=<ruta> si una ruta empieza con '-'.\nDentro de un repositorio de criterio (felix.toml) los comandos no necesitan rutas: [--criterio <dir>] [--universo <nombre>].\n";

/// Ayuda completa, mostrada en `stdout` con código de salida 0.
const AYUDA: &str = "CLI de Felix Felicis\n\nUso:\n  universo atomos <raiz> --manifiesto <manifiesto.toml>\n  universo validate <raiz> --manifiesto <manifiesto.toml> [--format <text|json>]\n  universo verificar <raiz> --manifiesto <manifiesto.toml> [--format <text|json>]\n  universo check <raiz> --manifiesto <manifiesto.toml> --invariantes <archivo> [--explain <decide|enumerate> [--rules] | --witness [--witness-limit <n>]] [--format <text|json>]\n  universo propose <raiz> --manifiesto <m> --invariantes <i> --delta <d.json> [--format text|json] [--explain decide|enumerate [--rules]] [--witness [all] [--witness-limit <n>]] [--emit-snapshot-token | --apply --expect-snapshot <token> [--author <a>] [--reason <r>]]\n  universo status <raiz> [--format <text|json>]\n  universo recover <raiz> [--format <text|json>]\n  universo guard <raiz> --manifiesto <m> --invariantes <i> [--format <text|json>]\n  universo seal <raiz> --manifiesto <m> --invariantes <i> [--autor <a>] [--motivo <m>] [--format <text|json>]\n  universo init <directorio> [--git] [--format <text|json>]\n  universo log <raiz> [--format <text|json>]\n  universo show <raiz> <n> [--format <text|json>]\n  universo diff <raiz> <desde> [<hasta>] [--format <text|json>]\n  universo delta <raiz> --desde <borrador> --manifiesto <m>\n  universo search <raiz> --manifiesto <m> <términos...> [--limit <n>] [--format <text|json>]\n  universo capturar <texto> --proyecto <nombre> [--tarea <t>] [--validacion <v>] [--dry-run]\n  universo reforzar|objetar <nota> --proyecto <nombre> --contexto <c> [--validacion <v>] [--dry-run]\n  universo promover <nota> --a <universo> [--de <universo>] [--dry-run]\n  universo mcp-config [--cliente claude-code|codex|json] [--nombre <servidor>]\n  universo conectar [--aplicar | --verificar] [--format text|json]\n  universo grafo [--universo <nombre>] [--salida <archivo.html>|-]\n  universo metricas [--hasta AAAA-MM-DD] [--estancada-dias <n>] [--format text|json]\n  universo resumen [--criterio <dir>] [--format <text|json>]\n  universo relacion <origen> <clave> <destino> [--quitar] | atributo <nota> <clave> <valor> [--quitar] | nota editar <nota>\n  universo reglas aplicar <archivo.sx> | manifiesto aplicar <archivo.toml> [--autor <a>] [--motivo <m>] [--dry-run]\n\nComandos:\n  atomos     Imprime los átomos del bundle en orden canónico.\n  validate   Valida el manifiesto, el formato OKF y la estructura del bundle.\n  verificar  Alias compatible de validate.\n  check      Evalúa el programa de invariantes sobre el bundle.\n  propose    Simula por defecto; --apply publica con snapshot esperado.\n  status     Informa, sin escribir, si hay una transacción pendiente (0 limpio, 1 pendiente).\n  recover    Toma el lock y completa o revierte una transacción interrumpida.\n  guard      Valida, evalúa invariantes y compara con el sello del último estado admitido (0 sólo si todo coincide).\n  seal       Acepta el estado actual como admitido si cumple las invariantes; si difiere del sello anterior, lo registra como cambio externo.\n  init       Crea un repositorio de conocimiento mínimo, válido y sellado; con --git, un repositorio de criterio con universos candidatos y vigente.\n  log        Muestra fecha, autor, motivo y sello de cada cambio aplicado.\n  show       Muestra el cambio n del log con los hechos que agregó y retiró (su salida incluye datos del bundle).\n  diff       Muestra los cambios netos entre dos estados del log: el que dejó la entrada <desde> (0: antes de la primera) y el que dejó <hasta> (por defecto, la última); su salida incluye datos del bundle.\n  delta      Calcula el delta que lleva el root admitido a un borrador editado (su salida incluye datos del bundle).\n  capturar   Guarda una afirmación de criterio como candidata, con su proyecto y contexto (repositorio de criterio).\n  reforzar   Registra que un criterio se sostuvo en un proyecto y lo confirma allí.\n  objetar    Registra que un criterio no se sostuvo en un proyecto.\n  promover   Lleva un criterio a otro universo del repositorio; las reglas del destino deciden.\n  relacion   Agrega o quita una relación entre dos notas.\n  atributo   Cambia, agrega o quita un valor de un atributo de una nota.\n  reglas     `reglas aplicar <archivo>` cambia las reglas del universo si el bundle actual las cumple y lo registra en el historial.\n  manifiesto `manifiesto aplicar <archivo>` cambia el manifiesto del universo con la misma comprobación.\n  resumen    Por universo: notas, estados, relaciones, reglas, listas para promover, candidatas o vigentes con objeciones y últimos cambios.\n  mcp-config Imprime la configuración del MCP para conectar un agente a este repositorio de criterio.\n  metricas   Mide el uso del criterio: capturas, refuerzos, objeciones y promociones por autor y semana, precisión del agente, días hasta la promoción, estancadas y uso del MCP.\n  grafo      Genera una página HTML local con el grafo de los universos: notas, relaciones, promociones, listas para promover y objetadas (incluye datos del bundle; no usa la red).\n  conectar   Conecta el repositorio de criterio a Claude Code (hooks con la ruta del binario, MCP de usuario y skill); sin --aplicar sólo muestra qué cambiaría.\n  nota       `nota editar <nota>` abre la nota en $VISUAL o $EDITOR y aplica lo que cambie.\n  search     Busca notas donde cada término empieza alguna palabra, sin distinguir mayúsculas ni tildes; `--` antes de términos que empiezan con '-' (su salida incluye datos del bundle).\n\nOpciones:\n  --manifiesto <ruta> | --manifiesto=<ruta>  Manifiesto TOML requerido.\n  --invariantes <ruta> | --invariantes=<ruta> Programa de invariantes requerido por check.\n  --explain decide|enumerate                  Explicación redactada opt-in de check y de propose (sólo simulación).\n  --rules                                     Con --explain: id y mensaje de cada regla en el JSON (check pasa a schema v3).\n  --witness                                   Con check: cada regla que falla y todas sus asignaciones falsas. Con propose (simulación): la primera asignación que incumple una regla; `--witness all`, todas. Revela rutas del bundle; sólo uso local.\n  --witness-limit <n>                         Con --witness (check) o --witness all (propose): asignaciones falsas que se muestran en total (por defecto 100; 0 lista sólo las reglas, sin rutas).\n  --format text|json                         Formato de salida de todos los comandos salvo atomos y delta (por defecto: text).\n  -h, --help                                 Muestra esta ayuda y termina con código 0.\n  -V, --version                              Muestra la versión y termina con código 0.\n  --criterio <dir>                           Repositorio de criterio (felix.toml); también FELIX_CRITERIO o el felix.toml más cercano hacia arriba.\n  --universo <nombre>                        Universo del repositorio (también FELIX_UNIVERSO); por defecto, el de felix.toml.\n  --ignorar-version                          Sigue aunque la versión de universo no cumpla el rango motor de felix.toml.\n\nSi la ruta de un archivo empieza con `-`, usa `--opcion=<ruta>`.\nSi la raíz posicional empieza con `-`, antepone `./` (por ejemplo, `./-bundle`).\nvalidate es una comprobación estructural; no evalúa invariantes.\ncheck informa admit/reject/error y no incluye datos concretos del bundle, salvo con --witness (rutas de las asignaciones que fallan). La explicación es redactada; su pase opt-in usa un presupuesto de 1.000.000 unidades además del pase completo que determina el resumen.\n\nCódigos de salida:\n  0  Bundle válido, check/propose admit, search completada (aunque sin coincidencias) o ayuda mostrada.\n  1  Bundle inválido, repositorio de criterio inválido o sin el universo pedido, check/propose reject/error, status pendiente/missing, guard fallido, seal/recover/delta/diff fallido o fallo de escritura.\n  2  Uso inválido.\n";

/// Formato de salida de `validate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Formato {
    Texto,
    Json,
}

/// Comando reconocido por la CLI, ya con sus argumentos resueltos.
enum Comando {
    Ayuda,
    Version,
    Propose(propose::Solicitud),
    Estado(estado::Solicitud),
    Guardia(guardia::Solicitud),
    Init(init::Solicitud),
    Log(historial::Solicitud),
    Show(historial::SolicitudShow),
    Search(busqueda::Solicitud),
    Diff(diff::Solicitud),
    Captura(captura_criterio::Solicitud),
    Promover(promocion::Solicitud),
    Edicion(edicion::Solicitud),
    Configuracion(configuracion::Solicitud),
    McpConfig(criterio::SolicitudMcpConfig),
    Conectar(conectar::Solicitud),
    Grafo(grafo::Solicitud),
    Metricas(metricas::Solicitud),
    Resumen(resumen::Solicitud),
    Delta(diferencia::Solicitud),
    Atomos {
        raiz: PathBuf,
        manifiesto: PathBuf,
    },
    Validate {
        raiz: PathBuf,
        manifiesto: PathBuf,
        formato: Formato,
    },
    Check {
        raiz: PathBuf,
        manifiesto: PathBuf,
        invariantes: PathBuf,
        formato: Formato,
        explicacion: Option<ModoExplicacion>,
        con_reglas: bool,
        testigos: Option<usize>,
    },
}

struct SolicitudCheck<'a> {
    raiz: &'a Path,
    manifiesto: &'a Path,
    invariantes: &'a Path,
    formato: Formato,
    explicacion: Option<ModoExplicacion>,
    /// `--rules`: JSON v3 con `rule_id`/`rule_message` por objeto.
    con_reglas: bool,
    /// `--witness [--witness-limit N]`: reglas que fallan y hasta N
    /// asignaciones falsas en total (revela rutas del bundle).
    testigos: Option<usize>,
}

/// Interpreta los argumentos de línea de comandos (sin el nombre del
/// binario). Acepta `--manifiesto <valor>` y `--manifiesto=<valor>`.
fn parsear_args(args: &[String]) -> Result<Comando, String> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        return Ok(Comando::Ayuda);
    }
    if matches!(args, [arg] if arg == "--version" || arg == "-V") {
        return Ok(Comando::Version);
    }

    let (nombre_comando, resto) = args.split_first().ok_or_else(|| USO.to_string())?;
    if nombre_comando == "propose" {
        return propose::parsear(resto).map(Comando::Propose);
    }
    if nombre_comando == "status" {
        return estado::parsear(estado::Accion::Status, resto).map(Comando::Estado);
    }
    if nombre_comando == "recover" {
        return estado::parsear(estado::Accion::Recover, resto).map(Comando::Estado);
    }
    if nombre_comando == "guard" {
        return guardia::parsear(guardia::Accion::Guard, resto).map(Comando::Guardia);
    }
    if nombre_comando == "seal" {
        return guardia::parsear(guardia::Accion::Seal, resto).map(Comando::Guardia);
    }
    if nombre_comando == "init" {
        return init::parsear(resto).map(Comando::Init);
    }
    if nombre_comando == "log" {
        return historial::parsear(resto).map(Comando::Log);
    }
    if nombre_comando == "show" {
        return historial::parsear_show(resto).map(Comando::Show);
    }
    let accion = match nombre_comando.as_str() {
        "capturar" => Some(captura_criterio::Accion::Capturar),
        "reforzar" => Some(captura_criterio::Accion::Reforzar),
        "objetar" => Some(captura_criterio::Accion::Objetar),
        _ => None,
    };
    if let Some(accion) = accion {
        return captura_criterio::parsear(accion, resto).map(Comando::Captura);
    }
    if nombre_comando == "resumen" {
        return resumen::parsear(resto).map(Comando::Resumen);
    }
    if nombre_comando == "metricas" {
        return metricas::parsear(resto).map(Comando::Metricas);
    }
    if nombre_comando == "grafo" {
        return grafo::parsear(resto).map(Comando::Grafo);
    }
    if nombre_comando == "conectar" {
        return conectar::parsear(resto).map(Comando::Conectar);
    }
    if nombre_comando == "mcp-config" {
        return criterio::parsear_mcp_config(resto).map(Comando::McpConfig);
    }
    if nombre_comando == "promover" {
        return promocion::parsear(resto).map(Comando::Promover);
    }
    if matches!(nombre_comando.as_str(), "relacion" | "atributo" | "nota") {
        return edicion::parsear(nombre_comando, resto).map(Comando::Edicion);
    }
    if matches!(nombre_comando.as_str(), "reglas" | "manifiesto") {
        return configuracion::parsear(nombre_comando, resto).map(Comando::Configuracion);
    }
    if nombre_comando == "diff" {
        return diff::parsear(resto).map(Comando::Diff);
    }
    if nombre_comando == "search" {
        return busqueda::parsear(resto).map(Comando::Search);
    }
    if nombre_comando == "delta" {
        return diferencia::parsear(resto).map(Comando::Delta);
    }
    let tipo_comando = match nombre_comando.as_str() {
        "atomos" => 0,
        "validate" | "verificar" => 1,
        "check" => 2,
        "-h" | "--help" => return Ok(Comando::Ayuda),
        _ => return Err(USO.to_string()),
    };

    let mut raiz: Option<PathBuf> = None;
    let mut manifiesto: Option<PathBuf> = None;
    let mut invariantes: Option<PathBuf> = None;
    let mut formato: Option<Formato> = None;
    let mut explicacion: Option<ModoExplicacion> = None;
    let mut con_reglas = false;
    let mut con_testigos = false;
    let mut limite_testigos: Option<usize> = None;
    let mut i = 0;
    while i < resto.len() {
        let arg = &resto[i];
        if matches!(arg.as_str(), "-h" | "--help") {
            return Ok(Comando::Ayuda);
        } else if let Some(valor) = arg.strip_prefix("--manifiesto=") {
            if manifiesto.is_some() || valor.is_empty() {
                return Err(USO.to_string());
            }
            manifiesto = Some(PathBuf::from(valor));
            i += 1;
        } else if arg == "--manifiesto" {
            let valor = resto.get(i + 1).ok_or_else(|| USO.to_string())?;
            if manifiesto.is_some() || valor.is_empty() || valor.starts_with('-') {
                return Err(USO.to_string());
            }
            manifiesto = Some(PathBuf::from(valor));
            i += 2;
        } else if let Some(valor) = arg.strip_prefix("--invariantes=") {
            if tipo_comando != 2 || invariantes.is_some() || valor.is_empty() {
                return Err(USO.to_string());
            }
            invariantes = Some(PathBuf::from(valor));
            i += 1;
        } else if arg == "--invariantes" {
            let valor = resto.get(i + 1).ok_or_else(|| USO.to_string())?;
            if tipo_comando != 2
                || invariantes.is_some()
                || valor.is_empty()
                || valor.starts_with('-')
            {
                return Err(USO.to_string());
            }
            invariantes = Some(PathBuf::from(valor));
            i += 2;
        } else if arg == "--witness" {
            if tipo_comando != 2 || con_testigos {
                return Err(USO.to_string());
            }
            con_testigos = true;
            i += 1;
        } else if arg == "--witness-limit" || arg.starts_with("--witness-limit=") {
            let (valor, avance) = match arg.strip_prefix("--witness-limit=") {
                Some(valor) => (valor, 1),
                None => (resto.get(i + 1).ok_or_else(|| USO.to_string())?.as_str(), 2),
            };
            if tipo_comando != 2 || limite_testigos.is_some() {
                return Err(USO.to_string());
            }
            limite_testigos = Some(valor.parse().map_err(|_| USO.to_string())?);
            i += avance;
        } else if arg == "--rules" {
            if tipo_comando != 2 || con_reglas {
                return Err(USO.to_string());
            }
            con_reglas = true;
            i += 1;
        } else if arg == "--explain" {
            let valor = resto.get(i + 1).ok_or_else(|| USO.to_string())?;
            if tipo_comando != 2 || explicacion.is_some() || valor.starts_with('-') {
                return Err(USO.to_string());
            }
            explicacion = Some(match valor.as_str() {
                "decide" => ModoExplicacion::Decidir,
                "enumerate" => ModoExplicacion::enumerar_por_defecto(),
                _ => return Err(USO.to_string()),
            });
            i += 2;
        } else if let Some(valor) = arg.strip_prefix("--format=") {
            if tipo_comando == 0 || formato.is_some() {
                return Err(USO.to_string());
            }
            formato = Some(parsear_formato(valor)?);
            i += 1;
        } else if arg == "--format" {
            let valor = resto.get(i + 1).ok_or_else(|| USO.to_string())?;
            if tipo_comando == 0 || formato.is_some() || valor.starts_with('-') {
                return Err(USO.to_string());
            }
            formato = Some(parsear_formato(valor)?);
            i += 2;
        } else if arg.starts_with('-') {
            return Err(USO.to_string());
        } else if raiz.is_none() {
            raiz = Some(PathBuf::from(arg));
            i += 1;
        } else {
            return Err(USO.to_string());
        }
    }

    if con_reglas && explicacion.is_none() {
        return Err(USO.to_string());
    }
    // --witness-limit exige --witness; --witness no se combina con --explain.
    if limite_testigos.is_some() && !con_testigos || con_testigos && explicacion.is_some() {
        return Err(USO.to_string());
    }
    let testigos = con_testigos.then(|| limite_testigos.unwrap_or(testigos::LIMITE_POR_OMISION));
    let raiz = raiz.ok_or_else(|| USO.to_string())?;
    let manifiesto = manifiesto.ok_or_else(|| USO.to_string())?;
    match tipo_comando {
        0 => Ok(Comando::Atomos { raiz, manifiesto }),
        1 => Ok(Comando::Validate {
            raiz,
            manifiesto,
            formato: formato.unwrap_or(Formato::Texto),
        }),
        _ => Ok(Comando::Check {
            raiz,
            manifiesto,
            invariantes: invariantes.ok_or_else(|| USO.to_string())?,
            formato: formato.unwrap_or(Formato::Texto),
            explicacion,
            con_reglas,
            testigos,
        }),
    }
}

fn parsear_formato(valor: &str) -> Result<Formato, String> {
    match valor {
        "text" => Ok(Formato::Texto),
        "json" => Ok(Formato::Json),
        _ => Err(USO.to_string()),
    }
}

/// Informa un fallo de escritura. Si el escritor proporcionado también falla,
/// intenta dejar una última nota en el `stderr` real del proceso.
fn reportar_fallo_escritura(error: &mut impl Write, destino: &str, causa: &io::Error) {
    let mensaje = format!("error al escribir {destino}: {causa}\n");
    if error.write_all(mensaje.as_bytes()).is_err() {
        let _ = io::stderr().write_all(mensaje.as_bytes());
    }
}

fn escribir_salida(
    salida: &mut impl Write,
    error: &mut impl Write,
    bytes: &[u8],
    destino: &str,
) -> bool {
    match salida.write_all(bytes) {
        Ok(()) => true,
        Err(causa) => {
            reportar_fallo_escritura(error, destino, &causa);
            false
        }
    }
}

/// Carga el manifiesto o escribe el error de carga en `error` y devuelve el
/// código de salida 1.
fn cargar_manifiesto_o_falla(ruta: &Path, error: &mut impl Write) -> Result<Manifiesto, i32> {
    cargar_manifiesto(ruta).map_err(|e| {
        let mensaje = format!("{e}\n");
        if let Err(causa) = error.write_all(mensaje.as_bytes()) {
            reportar_fallo_escritura(error, "stderr", &causa);
        }
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
    if let Err(causa) = error.write_all(buffer.as_bytes()) {
        reportar_fallo_escritura(error, "stderr", &causa);
    }
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
            if escribir_salida(salida, error, buffer.as_bytes(), "stdout") {
                0
            } else {
                1
            }
        }
        Err(errores) => reportar_errores(&errores, error),
    }
}

fn error_json(codigo: &str, mensaje: String, ruta: Option<String>, linea: Option<usize>) -> Value {
    json!({
        "code": codigo,
        "message": mensaje,
        "path": ruta,
        "line": linea,
    })
}

fn resultado_json_validate(status: &str, summary: Option<Value>, errors: Vec<Value>) -> Value {
    json!({
        "schema_version": 1,
        "command": "validate",
        "status": status,
        "summary": summary,
        "errors": errors,
    })
}

fn escribir_json_validate(
    status: &str,
    summary: Option<Value>,
    errors: Vec<Value>,
    salida: &mut impl Write,
    error: &mut impl Write,
    codigo_salida: i32,
) -> i32 {
    let mut bytes = match serde_json::to_vec(&resultado_json_validate(status, summary, errors)) {
        Ok(bytes) => bytes,
        Err(causa) => {
            let causa = io::Error::other(causa);
            reportar_fallo_escritura(error, "JSON", &causa);
            return 1;
        }
    };
    bytes.push(b'\n');
    if escribir_salida(salida, error, &bytes, "stdout") {
        codigo_salida
    } else {
        1
    }
}

fn error_manifiesto_json(ruta: &Path, error: &ErrorManifiestoArchivo) -> Value {
    let codigo = match error {
        ErrorManifiestoArchivo::Io(_) => "ManifestIo",
        ErrorManifiestoArchivo::Sintaxis(_) => "ManifestSyntax",
        ErrorManifiestoArchivo::Validacion(_) => "ManifestInvalid",
    };
    error_json(
        codigo,
        error.to_string(),
        Some(ruta.display().to_string()),
        None,
    )
}

fn errores_bundle_json(errores: &[ErrorBundle]) -> Vec<Value> {
    errores
        .iter()
        .map(|error| {
            error_json(
                &error.regla.to_string(),
                error.detalle.clone(),
                error.path.clone(),
                error.linea,
            )
        })
        .collect()
}

/// Ejecuta `validate`: solo carga el manifiesto y valida el formato OKF y la
/// estructura del bundle; nunca evalúa invariantes.
fn ejecutar_validate(
    raiz: &Path,
    manifiesto: &Path,
    formato: Formato,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    let manifiesto_cargado = match cargar_manifiesto(manifiesto) {
        Ok(m) => m,
        Err(error_manifiesto) => {
            if formato == Formato::Json {
                return escribir_json_validate(
                    "invalid",
                    None,
                    vec![error_manifiesto_json(manifiesto, &error_manifiesto)],
                    salida,
                    error,
                    1,
                );
            }
            let mensaje = format!("{error_manifiesto}\n");
            if let Err(causa) = error.write_all(mensaje.as_bytes()) {
                reportar_fallo_escritura(error, "stderr", &causa);
            }
            return 1;
        }
    };

    match cargar_bundle(raiz, manifiesto_cargado) {
        Ok(estructura) => match formato {
            Formato::Texto => {
                let mensaje = format!(
                    "OK: {} nodos, {} aristas\n",
                    estructura.len_nodos(),
                    estructura.len_aristas()
                );
                if escribir_salida(salida, error, mensaje.as_bytes(), "stdout") {
                    0
                } else {
                    1
                }
            }
            Formato::Json => escribir_json_validate(
                "valid",
                Some(json!({
                    "nodes": estructura.len_nodos(),
                    "edges": estructura.len_aristas(),
                })),
                Vec::new(),
                salida,
                error,
                0,
            ),
        },
        Err(errores) => match formato {
            Formato::Texto => reportar_errores(&errores, error),
            Formato::Json => escribir_json_validate(
                "invalid",
                None,
                errores_bundle_json(&errores),
                salida,
                error,
                1,
            ),
        },
    }
}

struct DiagnosticoCheck {
    kind: &'static str,
    code: String,
    message: &'static str,
    path: Option<String>,
    line: Option<usize>,
    column: Option<usize>,
    location: Option<String>,
}

impl DiagnosticoCheck {
    fn nuevo(
        kind: &'static str,
        code: impl Into<String>,
        message: &'static str,
        path: Option<String>,
        line: Option<usize>,
        column: Option<usize>,
        location: Option<String>,
    ) -> Self {
        Self {
            kind,
            code: code.into(),
            message,
            path,
            line,
            column,
            location,
        }
    }

    fn a_json(&self) -> Value {
        json!({
            "kind": self.kind,
            "code": self.code,
            "message": self.message,
            "path": self.path,
            "line": self.line,
            "column": self.column,
            "location": self.location,
        })
    }

    fn a_json_redactado(&self) -> Value {
        json!({
            "kind": self.kind,
            "code": self.code,
            "message": self.message,
            "path": null,
            "line": self.line,
            "column": self.column,
            "location": null,
        })
    }

    fn a_texto(&self) -> String {
        let mut ubicacion = self.path.as_deref().unwrap_or("check").to_string();
        if let Some(linea) = self.line {
            ubicacion.push_str(&format!(":{linea}"));
            if let Some(columna) = self.column {
                ubicacion.push_str(&format!(":{columna}"));
            }
        }
        format!("{ubicacion}: {}: {}\n", self.code, self.message)
    }

    fn a_texto_redactado(&self) -> String {
        let mut ubicacion = "check".to_string();
        if let Some(linea) = self.line {
            ubicacion.push_str(&format!(":{linea}"));
            if let Some(columna) = self.column {
                ubicacion.push_str(&format!(":{columna}"));
            }
        }
        format!("{ubicacion}: {}: {}\n", self.code, self.message)
    }
}

fn resultado_json_check(
    status: &str,
    summary: Option<Value>,
    errors: &[DiagnosticoCheck],
) -> Value {
    json!({
        "schema_version": 1,
        "command": "check",
        "status": status,
        "summary": summary,
        "errors": errors.iter().map(DiagnosticoCheck::a_json).collect::<Vec<_>>(),
    })
}

fn resumen_check(sources: usize, instances: usize, evaluated: usize, failed: usize) -> Value {
    json!({
        "sources": sources,
        "instances": instances,
        "evaluated": evaluated,
        "failed": failed,
    })
}

fn escribir_resultado_check(
    status: &str,
    summary: Option<Value>,
    errors: &[DiagnosticoCheck],
    formato: Formato,
    codigo_salida: i32,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    if formato == Formato::Json {
        let mut bytes = match serde_json::to_vec(&resultado_json_check(status, summary, errors)) {
            Ok(bytes) => bytes,
            Err(causa) => {
                let causa = io::Error::other(causa);
                reportar_fallo_escritura(error, "JSON", &causa);
                return 1;
            }
        };
        bytes.push(b'\n');
        return if escribir_salida(salida, error, &bytes, "stdout") {
            codigo_salida
        } else {
            1
        };
    }

    if status == "error" {
        let bytes: String = errors.iter().map(DiagnosticoCheck::a_texto).collect();
        if let Err(causa) = error.write_all(bytes.as_bytes()) {
            reportar_fallo_escritura(error, "stderr", &causa);
        }
        return codigo_salida;
    }

    let summary = summary.expect("admit/reject siempre tiene resumen");
    let sources = summary["sources"].as_u64().unwrap_or_default();
    let instances = summary["instances"].as_u64().unwrap_or_default();
    let evaluated = summary["evaluated"].as_u64().unwrap_or_default();
    let failed = summary["failed"].as_u64().unwrap_or_default();
    let mensaje = format!(
        "{}: {sources} fuentes, {instances} instancias, {evaluated} evaluadas, {failed} fallidas\n",
        status.to_ascii_uppercase()
    );
    if escribir_salida(salida, error, mensaje.as_bytes(), "stdout") {
        codigo_salida
    } else {
        1
    }
}

fn escribir_error_check(
    errores: &[DiagnosticoCheck],
    formato: Formato,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    escribir_resultado_check("error", None, errores, formato, 1, salida, error)
}

fn resultado_json_check_explicado(
    status: &str,
    summary: Option<Value>,
    errors: &[DiagnosticoCheck],
    explanations: Option<Value>,
    schema_version: u8,
) -> Value {
    json!({
        "schema_version": schema_version,
        "command": "check",
        "status": status,
        "summary": summary,
        "errors": errors.iter().map(DiagnosticoCheck::a_json_redactado).collect::<Vec<_>>(),
        "explanations": explanations,
    })
}

struct ResultadoCheckExplicado<'a> {
    status: &'a str,
    summary: Option<Value>,
    errors: &'a [DiagnosticoCheck],
    explanations: Option<Value>,
    /// Id y mensaje de cada fuente (texto opt-in y JSON v3).
    reglas: Vec<(Option<String>, Option<String>)>,
    /// `--rules`: el JSON pasa a v3 y cada objeto lleva `rule_id`/`rule_message`.
    con_reglas: bool,
}

fn escribir_resultado_check_explicado(
    resultado: ResultadoCheckExplicado<'_>,
    formato: Formato,
    codigo_salida: i32,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    if formato == Formato::Json {
        let mut explanations = resultado.explanations;
        if resultado.con_reglas {
            if let Some(explanations) = explanations.as_mut() {
                anotar_reglas(explanations, &resultado.reglas);
            }
        }
        let mut bytes = match serde_json::to_vec(&resultado_json_check_explicado(
            resultado.status,
            resultado.summary,
            resultado.errors,
            explanations,
            if resultado.con_reglas { 3 } else { 2 },
        )) {
            Ok(bytes) => bytes,
            Err(causa) => {
                let causa = io::Error::other(causa);
                reportar_fallo_escritura(error, "JSON", &causa);
                return 1;
            }
        };
        bytes.push(b'\n');
        return if escribir_salida(salida, error, &bytes, "stdout") {
            codigo_salida
        } else {
            1
        };
    }

    if resultado.status == "error" {
        let bytes: String = resultado
            .errors
            .iter()
            .map(DiagnosticoCheck::a_texto_redactado)
            .collect();
        if let Err(causa) = error.write_all(bytes.as_bytes()) {
            reportar_fallo_escritura(error, "stderr", &causa);
        }
        return codigo_salida;
    }

    let summary = resultado
        .summary
        .expect("admit/reject siempre tiene resumen");
    let sources = summary["sources"].as_u64().unwrap_or_default();
    let instances = summary["instances"].as_u64().unwrap_or_default();
    let evaluated = summary["evaluated"].as_u64().unwrap_or_default();
    let failed = summary["failed"].as_u64().unwrap_or_default();
    let mut mensaje = format!(
        "{}: {sources} fuentes, {instances} instancias, {evaluated} evaluadas, {failed} fallidas\n",
        resultado.status.to_ascii_uppercase()
    );
    if let Some(explanations) = resultado.explanations {
        mensaje.push_str(&explicaciones_a_texto(&explanations, &resultado.reglas));
    }
    if escribir_salida(salida, error, mensaje.as_bytes(), "stdout") {
        codigo_salida
    } else {
        1
    }
}

fn escribir_error_check_explicado(
    errores: &[DiagnosticoCheck],
    formato: Formato,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    escribir_resultado_check_explicado(
        ResultadoCheckExplicado {
            status: "error",
            summary: None,
            errors: errores,
            explanations: None,
            reglas: Vec::new(),
            con_reglas: false,
        },
        formato,
        1,
        salida,
        error,
    )
}

fn escribir_error_check_segun_modo(
    errores: &[DiagnosticoCheck],
    formato: Formato,
    modo: Option<ModoExplicacion>,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    if modo.is_some() {
        escribir_error_check_explicado(errores, formato, salida, error)
    } else {
        escribir_error_check(errores, formato, salida, error)
    }
}

/// `reglas[i]`: id y mensaje opcionales declarados con `(regla ...)` para la
/// fuente `source_index == i`; se muestran sólo en este texto opt-in.
fn explicaciones_a_texto(
    explanations: &Value,
    reglas: &[(Option<String>, Option<String>)],
) -> String {
    let mode = explanations["mode"].as_str().unwrap_or("unknown");
    let objects = explanations["objects"].as_array().map_or(0, Vec::len);
    let truncated = explanations["truncated"].as_bool().unwrap_or(false);
    let reason = explanations["reason"].as_str().unwrap_or("none");
    let budget = &explanations["budget"];
    let mut output = format!(
        "EXPLANATIONS mode={mode} objects={objects} truncated={truncated} reason={reason}\n"
    );
    output.push_str(&format!(
        "  budget total={} limit={} semantic={} evidence={} aggregator={}\n",
        budget["consumed"].as_u64().unwrap_or_default(),
        budget["limit"].as_u64().unwrap_or_default(),
        budget["semantic_units"].as_u64().unwrap_or_default(),
        budget["evidence_units"].as_u64().unwrap_or_default(),
        budget["aggregator_units"].as_u64().unwrap_or_default(),
    ));
    for object in explanations["objects"].as_array().into_iter().flatten() {
        output.push_str(&format!(
            "  object source={} line={} column={} instance={}\n",
            object["source_index"].as_u64().unwrap_or_default(),
            object["line"]
                .as_u64()
                .map_or_else(|| "none".to_string(), |n| n.to_string()),
            object["column"]
                .as_u64()
                .map_or_else(|| "none".to_string(), |n| n.to_string()),
            object["instance_index"]
                .as_u64()
                .map_or_else(|| "none".to_string(), |n| n.to_string()),
        ));
        let regla = object["source_index"]
            .as_u64()
            .and_then(|i| reglas.get(usize::try_from(i).ok()?));
        if let Some((id, mensaje)) = regla {
            if id.is_some() || mensaje.is_some() {
                output.push_str(&format!(
                    "    rule: id={} message={}\n",
                    id.as_deref().unwrap_or("none"),
                    mensaje.as_deref().unwrap_or("none")
                ));
            }
        }
        output.push_str("    counterexample: ");
        output.push_str(&object["counterexample"].to_string());
        output.push('\n');
        output.push_str("    presence_core: ");
        output.push_str(&object["presence_core"].to_string());
        output.push('\n');
        output.push_str("    gap: ");
        output.push_str(&object["gap"].to_string());
        output.push('\n');
    }
    output
}

fn error_manifiesto_check(ruta: &Path, error: &ErrorManifiestoArchivo) -> DiagnosticoCheck {
    let (kind, code, message) = match error {
        ErrorManifiestoArchivo::Io(_) => {
            ("resource", "manifest_io", "no se pudo leer el manifiesto")
        }
        ErrorManifiestoArchivo::Sintaxis(_) => {
            ("data", "manifest_syntax", "el manifiesto no es válido")
        }
        ErrorManifiestoArchivo::Validacion(_) => {
            ("data", "manifest_invalid", "el manifiesto no es válido")
        }
    };
    DiagnosticoCheck::nuevo(
        kind,
        code,
        message,
        Some(ruta.display().to_string()),
        None,
        None,
        None,
    )
}

fn errores_bundle_check(errores: &[ErrorBundle]) -> Vec<DiagnosticoCheck> {
    errores
        .iter()
        .map(|error| {
            DiagnosticoCheck::nuevo(
                if error.regla == Regla::Io {
                    "resource"
                } else {
                    "data"
                },
                error.regla.to_string(),
                "el bundle no pudo cargarse como estructura válida",
                error.path.clone(),
                error.linea,
                None,
                None,
            )
        })
        .collect()
}

fn error_autoria_check(ruta: &Path, error: &ErrorAutoria) -> DiagnosticoCheck {
    DiagnosticoCheck::nuevo(
        "authorship",
        "invariant_syntax",
        "el programa de invariantes no cumple la sintaxis",
        Some(ruta.display().to_string()),
        Some(error.linea),
        Some(error.columna),
        None,
    )
}

fn errores_validacion_check(
    ruta: &Path,
    fuentes: &[SentenciaFuente],
    manifiesto: &Manifiesto,
    estructura: &universo_core::Estructura,
) -> Vec<DiagnosticoCheck> {
    let mut errores = Vec::new();
    for fuente in fuentes {
        for error in validar_sentencia_fuente(fuente, manifiesto, estructura) {
            errores.push(DiagnosticoCheck::nuevo(
                "authorship",
                error.codigo.as_str(),
                "una sentencia no pasa la validación estática",
                Some(ruta.display().to_string()),
                error.linea,
                error.columna,
                Some(error.ubicacion),
            ));
        }
    }
    errores
}

fn errores_expansion_check(
    ruta: &Path,
    fuentes: &[SentenciaFuente],
    error: ErrorExpansion,
) -> Vec<DiagnosticoCheck> {
    match error {
        ErrorExpansion::SentenciaInvalida { indice, errores } => errores
            .into_iter()
            .map(|error| {
                DiagnosticoCheck::nuevo(
                    "authorship",
                    error.codigo.as_str(),
                    "una sentencia no pasa la validación estática",
                    Some(ruta.display().to_string()),
                    fuentes.get(indice).map(|fuente| fuente.linea),
                    fuentes.get(indice).map(|fuente| fuente.columna),
                    Some(error.ubicacion),
                )
            })
            .collect(),
        ErrorExpansion::DominioPredicadoDesconocido(_) => vec![DiagnosticoCheck::nuevo(
            "authorship",
            "schema_domain_unknown",
            "un esquema refiere a un dominio no disponible",
            Some(ruta.display().to_string()),
            None,
            None,
            None,
        )],
        ErrorExpansion::PresupuestoAgotado { .. } => vec![DiagnosticoCheck::nuevo(
            "resource",
            "expansion_budget_exceeded",
            "se agotó el presupuesto de expansión",
            Some(ruta.display().to_string()),
            None,
            None,
            None,
        )],
    }
}

fn error_evaluacion_check(ruta: &Path, error: ErrorEvaluacion) -> DiagnosticoCheck {
    let (kind, code, message) = match error {
        ErrorEvaluacion::PresupuestoAgotado { .. } => (
            "resource",
            "evaluation_budget_exceeded",
            "se agotó el presupuesto de evaluación",
        ),
        _ => (
            "authorship",
            "evaluation_invalid",
            "una fórmula no puede evaluarse en esta estructura",
        ),
    };
    DiagnosticoCheck::nuevo(
        kind,
        code,
        message,
        Some(ruta.display().to_string()),
        None,
        None,
        None,
    )
}

fn error_explicacion_check(error: ErrorVeredicto) -> DiagnosticoCheck {
    let (kind, code, message) = match error {
        ErrorVeredicto::PresupuestoAgotado { .. }
        | ErrorVeredicto::Evaluacion(
            ErrorEvaluacion::PresupuestoAgotado { .. }
            | ErrorEvaluacion::PresupuestoEvidenciaAgotado { .. },
        ) => (
            "resource",
            "explanation_budget_exceeded",
            "se agotó el presupuesto de explicación",
        ),
        ErrorVeredicto::Evaluacion(_) => (
            "authorship",
            "explanation_evaluation_invalid",
            "no se pudo construir evidencia para una instancia",
        ),
    };
    DiagnosticoCheck::nuevo(kind, code, message, None, None, None, None)
}

fn explicar_check(
    expansion: &universo_core::invariante::expansion::ResultadoExpansionFuente,
    estructura: &universo_core::Estructura,
    modo: ModoExplicacion,
    limite: u64,
) -> Result<Value, ErrorVeredicto> {
    explicar_con_presupuesto_y_consumos(expansion, estructura, modo, limite)
        .map(|medido| explicaciones_medidas_a_json(&medido, modo, limite))
}

/// Ejecuta el pipeline cerrado de `check`. Ningún error de carga, autoría o
/// presupuesto se convierte en un veredicto lógico `reject`.
fn ejecutar_check(
    solicitud: SolicitudCheck<'_>,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    // check no toma el lock: avisa por stderr si hay una transacción pendiente,
    // sin cambiar stdout ni el código de salida.
    if let Some(aviso) = estado::aviso_pendiente(solicitud.raiz) {
        if let Err(causa) = error.write_all(aviso.as_bytes()) {
            reportar_fallo_escritura(error, "stderr", &causa);
        }
    }
    ejecutar_check_con_presupuesto_explicacion(
        solicitud,
        PRESUPUESTO_EXPLICACION_POR_DEFECTO,
        salida,
        error,
    )
}

/// Inyección privada del límite para regresores deterministas de agotamiento.
/// La CLI usa siempre `PRESUPUESTO_EXPLICACION_POR_DEFECTO` mediante
/// [`ejecutar_check`]; no existe una opción pública para cambiarlo.
fn ejecutar_check_con_presupuesto_explicacion(
    solicitud: SolicitudCheck<'_>,
    limite_explicacion: u64,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    let SolicitudCheck {
        raiz,
        manifiesto,
        invariantes,
        formato,
        explicacion,
        con_reglas,
        testigos,
    } = solicitud;
    let manifiesto_cargado = match cargar_manifiesto(manifiesto) {
        Ok(manifiesto) => manifiesto,
        Err(error_manifiesto) => {
            let diagnostico = error_manifiesto_check(manifiesto, &error_manifiesto);
            return escribir_error_check_segun_modo(
                &[diagnostico],
                formato,
                explicacion,
                salida,
                error,
            );
        }
    };
    let estructura = match cargar_bundle(raiz, manifiesto_cargado) {
        Ok(estructura) => estructura,
        Err(errores) => {
            let diagnosticos = errores_bundle_check(&errores);
            return escribir_error_check_segun_modo(
                &diagnosticos,
                formato,
                explicacion,
                salida,
                error,
            );
        }
    };

    let bytes = match fs::read(invariantes) {
        Ok(bytes) => bytes,
        Err(_) => {
            let diagnostico = DiagnosticoCheck::nuevo(
                "resource",
                "invariants_io",
                "no se pudo leer el archivo de invariantes",
                Some(invariantes.display().to_string()),
                None,
                None,
                None,
            );
            return escribir_error_check_segun_modo(
                &[diagnostico],
                formato,
                explicacion,
                salida,
                error,
            );
        }
    };
    let texto = match String::from_utf8(bytes) {
        Ok(texto) => texto,
        Err(_) => {
            let diagnostico = DiagnosticoCheck::nuevo(
                "authorship",
                "invariants_not_utf8",
                "el archivo de invariantes no es UTF-8 válido",
                Some(invariantes.display().to_string()),
                None,
                None,
                None,
            );
            return escribir_error_check_segun_modo(
                &[diagnostico],
                formato,
                explicacion,
                salida,
                error,
            );
        }
    };
    let fuentes = match parsear_programa(&texto) {
        Ok(fuentes) => fuentes,
        Err(error_autoria) => {
            let diagnostico = error_autoria_check(invariantes, &error_autoria);
            return escribir_error_check_segun_modo(
                &[diagnostico],
                formato,
                explicacion,
                salida,
                error,
            );
        }
    };

    let diagnosticos =
        errores_validacion_check(invariantes, &fuentes, estructura.manifiesto(), &estructura);
    if !diagnosticos.is_empty() {
        return escribir_error_check_segun_modo(&diagnosticos, formato, explicacion, salida, error);
    }

    // El pase de explicación opt-in conserva fuente para la procedencia. El
    // pase baseline sigue evaluando el programa completo para conservar status
    // y summary; la explicación vuelve a evaluar trazas bajo su presupuesto
    // global independiente de 1.000.000 unidades.
    let expansion_fuente = if explicacion.is_some() || testigos.is_some() {
        match expandir_sentencias_fuente(&fuentes, estructura.manifiesto(), &estructura) {
            Ok(expansion) => Some(expansion),
            Err(error_expansion) => {
                let diagnosticos = errores_expansion_check(invariantes, &fuentes, error_expansion);
                return escribir_error_check_segun_modo(
                    &diagnosticos,
                    formato,
                    explicacion,
                    salida,
                    error,
                );
            }
        }
    } else {
        None
    };
    let expansion_legacy = if expansion_fuente.is_none() {
        let sentencias: Vec<_> = fuentes
            .iter()
            .map(|fuente| fuente.sentencia.clone())
            .collect();
        match expandir_sentencias(&sentencias, estructura.manifiesto(), &estructura) {
            Ok(expansion) => Some(expansion),
            Err(error_expansion) => {
                let diagnosticos = errores_expansion_check(invariantes, &fuentes, error_expansion);
                return escribir_error_check_segun_modo(
                    &diagnosticos,
                    formato,
                    explicacion,
                    salida,
                    error,
                );
            }
        }
    } else {
        None
    };
    let (oraciones, total_instancias): (Vec<&universo_core::Formula>, usize) =
        if let Some(expansion) = &expansion_fuente {
            (
                expansion
                    .oraciones
                    .iter()
                    .map(|oracion| &oracion.formula)
                    .collect(),
                expansion.oraciones.len(),
            )
        } else if let Some(expansion) = &expansion_legacy {
            (
                expansion.oraciones.iter().collect(),
                expansion.oraciones.len(),
            )
        } else {
            return escribir_error_check_segun_modo(
                &[DiagnosticoCheck::nuevo(
                    "resource",
                    "check_internal_error",
                    "no se pudo preparar el programa de invariantes",
                    None,
                    None,
                    None,
                    None,
                )],
                formato,
                explicacion,
                salida,
                error,
            );
        };

    let mut evaluadas = 0;
    let mut fallidas = 0;
    // Posiciones de las oraciones falsas, para nombrar sus reglas (--witness).
    let mut posiciones_falsas = Vec::new();
    for (posicion, formula) in oraciones.into_iter().enumerate() {
        match evaluar(formula, &estructura) {
            Ok(true) => evaluadas += 1,
            Ok(false) => {
                evaluadas += 1;
                fallidas += 1;
                posiciones_falsas.push(posicion);
            }
            Err(error_evaluacion) => {
                let diagnostico = error_evaluacion_check(invariantes, error_evaluacion);
                return escribir_error_check_segun_modo(
                    &[diagnostico],
                    formato,
                    explicacion,
                    salida,
                    error,
                );
            }
        }
    }
    let resumen = resumen_check(fuentes.len(), total_instancias, evaluadas, fallidas);
    let status = if fallidas == 0 { "admit" } else { "reject" };
    let codigo_salida = if fallidas == 0 { 0 } else { 1 };
    if let Some(modo) = explicacion {
        let Some(expansion) = expansion_fuente.as_ref() else {
            return escribir_error_check_explicado(
                &[DiagnosticoCheck::nuevo(
                    "resource",
                    "check_internal_error",
                    "no se pudo preparar la procedencia de las explicaciones",
                    None,
                    None,
                    None,
                    None,
                )],
                formato,
                salida,
                error,
            );
        };
        match explicar_check(expansion, &estructura, modo, limite_explicacion) {
            Ok(explanations) => escribir_resultado_check_explicado(
                ResultadoCheckExplicado {
                    status,
                    summary: Some(resumen),
                    errors: &[],
                    explanations: Some(explanations),
                    reglas: fuentes
                        .iter()
                        .map(|f| (f.id.clone(), f.mensaje.clone()))
                        .collect(),
                    con_reglas,
                },
                formato,
                codigo_salida,
                salida,
                error,
            ),
            Err(diagnostico) => {
                let diagnostico = error_explicacion_check(diagnostico);
                escribir_error_check_explicado(&[diagnostico], formato, salida, error)
            }
        }
    } else if let (Some(limite), Some(expansion)) = (testigos, expansion_fuente.as_ref()) {
        let falsas: Vec<_> = posiciones_falsas
            .iter()
            .map(|&p| expansion.oraciones[p].clone())
            .collect();
        let (encontrados, error_testigos) = if limite == 0 || falsas.is_empty() {
            (Vec::new(), None)
        } else {
            match universo_core::testigos_de_expansion(&falsas, &estructura, limite) {
                Ok(encontrados) => (encontrados, None),
                Err(universo_core::ErrorEvaluacion::PresupuestoAgotado { .. }) => {
                    (Vec::new(), Some("witness_budget_exceeded"))
                }
                Err(_) => (Vec::new(), Some("witness_failed")),
            }
        };
        let fuentes_falsas: Vec<usize> = falsas.iter().map(|o| o.source_index).collect();
        let reglas = testigos::agrupar(&fuentes, &fuentes_falsas, &encontrados);
        let con_asignaciones = limite > 0;
        let bytes = if formato == Formato::Json {
            let mut valor = resultado_json_check(status, Some(resumen), &[]);
            valor["failed_rules"] = testigos::a_json(&reglas, con_asignaciones);
            if con_asignaciones {
                valor["witness_error"] = json!(error_testigos);
            }
            format!("{valor}\n")
        } else {
            let mut texto = String::new();
            let _ = escribir_resultado_check(
                status,
                Some(resumen),
                &[],
                formato,
                codigo_salida,
                &mut CursorTexto(&mut texto),
                error,
            );
            texto.push_str(&testigos::a_texto(&reglas, con_asignaciones));
            if let Some(codigo) = error_testigos {
                texto.push_str(&format!("witness: {codigo}\n"));
            }
            texto
        };
        if escribir_salida(salida, error, bytes.as_bytes(), "stdout") {
            codigo_salida
        } else {
            1
        }
    } else {
        escribir_resultado_check(
            status,
            Some(resumen),
            &[],
            formato,
            codigo_salida,
            salida,
            error,
        )
    }
}

/// Acumula en un `String` lo que `escribir_resultado_check` escribe, para
/// agregar las reglas y los testigos debajo del resumen.
struct CursorTexto<'a>(&'a mut String);

impl Write for CursorTexto<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.push_str(&String::from_utf8_lossy(buf));
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Interpreta `args` y ejecuta el comando correspondiente, escribiendo en
/// `salida`/`error` y devolviendo el código de salida del proceso. Nunca
/// entra en pánico ante entradas de usuario inválidas.
pub(crate) fn ejecutar(args: Vec<String>, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    // Dentro de un repositorio de criterio, los comandos no necesitan rutas.
    let args = match criterio::preparar(args, USO) {
        criterio::Preparados::Listos(args) => args,
        criterio::Preparados::Uso(uso) => {
            if let Err(causa) = error.write_all(uso.as_bytes()) {
                reportar_fallo_escritura(error, "stderr", &causa);
            }
            return 2;
        }
        criterio::Preparados::BuscarTodos { universos, resto } => {
            return busqueda::ejecutar_todos(universos, &resto, salida, error);
        }
        criterio::Preparados::Error { texto, json } => {
            if json {
                escribir_salida(salida, error, texto.as_bytes(), "stdout");
            } else if let Err(causa) = error.write_all(texto.as_bytes()) {
                reportar_fallo_escritura(error, "stderr", &causa);
            }
            return 1;
        }
    };
    let comando = match parsear_args(&args) {
        Ok(comando) => comando,
        Err(uso) => {
            if let Err(causa) = error.write_all(uso.as_bytes()) {
                reportar_fallo_escritura(error, "stderr", &causa);
            }
            return 2;
        }
    };
    match comando {
        Comando::Ayuda => {
            if escribir_salida(salida, error, AYUDA.as_bytes(), "stdout") {
                0
            } else {
                1
            }
        }
        Comando::Version => {
            let texto = format!("universo {}\n", env!("CARGO_PKG_VERSION"));
            if escribir_salida(salida, error, texto.as_bytes(), "stdout") {
                0
            } else {
                1
            }
        }
        Comando::Propose(solicitud) => propose::ejecutar(solicitud, salida, error),
        Comando::Estado(solicitud) => estado::ejecutar(solicitud, salida, error),
        Comando::Guardia(solicitud) => guardia::ejecutar(solicitud, salida, error),
        Comando::Init(solicitud) => init::ejecutar(solicitud, salida, error),
        Comando::Log(solicitud) => historial::ejecutar(solicitud, salida, error),
        Comando::Show(solicitud) => historial::ejecutar_show(solicitud, salida, error),
        Comando::Delta(solicitud) => diferencia::ejecutar(solicitud, salida, error),
        Comando::Search(solicitud) => busqueda::ejecutar(solicitud, salida, error),
        Comando::Diff(solicitud) => diff::ejecutar(solicitud, salida, error),
        Comando::Captura(solicitud) => captura_criterio::ejecutar(solicitud, salida, error),
        Comando::Promover(solicitud) => promocion::ejecutar(solicitud, salida, error),
        Comando::Edicion(solicitud) => edicion::ejecutar(solicitud, salida, error),
        Comando::Configuracion(solicitud) => configuracion::ejecutar(solicitud, salida, error),
        Comando::McpConfig(solicitud) => criterio::ejecutar_mcp_config(solicitud, salida, error),
        Comando::Conectar(solicitud) => conectar::ejecutar(solicitud, salida, error),
        Comando::Grafo(solicitud) => grafo::ejecutar(solicitud, salida, error),
        Comando::Metricas(solicitud) => metricas::ejecutar(solicitud, salida, error),
        Comando::Resumen(solicitud) => resumen::ejecutar(solicitud, salida, error),
        Comando::Atomos { raiz, manifiesto } => ejecutar_atomos(&raiz, &manifiesto, salida, error),
        Comando::Validate {
            raiz,
            manifiesto,
            formato,
        } => ejecutar_validate(&raiz, &manifiesto, formato, salida, error),
        Comando::Check {
            raiz,
            manifiesto,
            invariantes,
            formato,
            explicacion,
            con_reglas,
            testigos,
        } => ejecutar_check(
            SolicitudCheck {
                raiz: &raiz,
                manifiesto: &manifiesto,
                invariantes: &invariantes,
                formato,
                explicacion,
                con_reglas,
                testigos,
            },
            salida,
            error,
        ),
    }
}

fn main() {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();

    let mut args: Vec<String> = Vec::new();
    let mut argumento_no_unicode = false;
    for arg in std::env::args_os().skip(1) {
        match arg.into_string() {
            Ok(s) => args.push(s),
            Err(_) => {
                argumento_no_unicode = true;
                break;
            }
        }
    }

    let codigo = if argumento_no_unicode {
        if let Err(causa) = stderr.write_all(USO.as_bytes()) {
            reportar_fallo_escritura(&mut stderr, "stderr", &causa);
        }
        2
    } else {
        ejecutar(args, &mut stdout, &mut stderr)
    };

    if let Err(causa) = stdout.flush() {
        reportar_fallo_escritura(&mut stderr, "stdout", &causa);
        let _ = stderr.flush();
        std::process::exit(1);
    }
    if stderr.flush().is_err() {
        std::process::exit(1);
    }
    std::process::exit(codigo);
}

#[cfg(test)]
mod tests {
    use super::*;
    use universo_testkit::bundle_sintetico::Constructor;

    const MANIFIESTO_PRUEBA: &str = r#"
version = "cli-budget-test/1"
[nodos]
sortes = ["a", "Retirado", "Patrón"]
[[aristas]]
clave = "rel"
simetrica = true
[[predicados]]
nombre = "Nivel"
clave = "nivel"
dominio = ["bajo", "alto"]
[layout]
directorios = ["n"]
"#;

    #[test]
    fn agotamiento_presupuestario_no_parcial_json_v2_para_ambos_modos() {
        let root = Constructor::nuevo()
            .archivo(
                "n/private-node.md",
                "---\ntype: a\ntitle: PRIVATE_LABEL_CANARY\n---\n",
            )
            .escribir();
        let manifiesto = root.path().join("manifest.toml");
        fs::write(&manifiesto, MANIFIESTO_PRUEBA).unwrap();
        let invariantes = root.path().join("invariants.smt2");
        fs::write(
            &invariantes,
            "(forall ((x N)) (not (Type x a)))\n(forall ((x N)) (not (Type x a)))\n",
        )
        .unwrap();

        for modo in [
            ModoExplicacion::Decidir,
            ModoExplicacion::enumerar_por_defecto(),
        ] {
            let mut salida = Vec::new();
            let mut error = Vec::new();
            let codigo = ejecutar_check_con_presupuesto_explicacion(
                SolicitudCheck {
                    raiz: root.path(),
                    manifiesto: &manifiesto,
                    invariantes: &invariantes,
                    formato: Formato::Json,
                    explicacion: Some(modo),
                    con_reglas: false,
                    testigos: None,
                },
                0,
                &mut salida,
                &mut error,
            );

            assert_eq!(codigo, 1);
            assert!(error.is_empty());
            assert!(salida.ends_with(b"\n"));
            let json: Value = serde_json::from_slice(&salida).unwrap();
            assert_eq!(
                json,
                json!({
                    "schema_version": 2,
                    "command": "check",
                    "status": "error",
                    "summary": null,
                    "errors": [{
                        "kind": "resource",
                        "code": "explanation_budget_exceeded",
                        "message": "se agotó el presupuesto de explicación",
                        "path": null,
                        "line": null,
                        "column": null,
                        "location": null,
                    }],
                    "explanations": null,
                })
            );
            assert!(!String::from_utf8_lossy(&salida).contains("PRIVATE_LABEL_CANARY"));

            let limite_inyectado = 4_096;
            let mut salida_con_limite = Vec::new();
            let mut error_con_limite = Vec::new();
            let codigo_con_limite = ejecutar_check_con_presupuesto_explicacion(
                SolicitudCheck {
                    raiz: root.path(),
                    manifiesto: &manifiesto,
                    invariantes: &invariantes,
                    formato: Formato::Json,
                    explicacion: Some(modo),
                    con_reglas: false,
                    testigos: None,
                },
                limite_inyectado,
                &mut salida_con_limite,
                &mut error_con_limite,
            );
            assert_eq!(codigo_con_limite, 1, "el check lógico sigue siendo reject");
            assert!(error_con_limite.is_empty());
            let json_con_limite: Value = serde_json::from_slice(&salida_con_limite).unwrap();
            assert_eq!(json_con_limite["status"], "reject");
            assert_eq!(json_con_limite["errors"], json!([]));
            assert!(!json_con_limite["summary"].is_null());
            assert!(!json_con_limite["explanations"].is_null());
            assert_eq!(
                json_con_limite["explanations"]["budget"]["limit"],
                limite_inyectado
            );
            assert!(
                json_con_limite["explanations"]["budget"]["consumed"]
                    .as_u64()
                    .unwrap()
                    <= limite_inyectado
            );
        }
    }
}
