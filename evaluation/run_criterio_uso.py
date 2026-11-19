#!/usr/bin/env python3
"""Evaluación del criterio (fase 11, gate ``v0.5.0``): ¿consultar el
repositorio de criterio hace que un agente decida como quiere el dueño?

Tres brazos con el mismo modelo, tarea y presupuesto:

- ``A_sin_criterio``: sólo la tarea.
- ``B_criterio_mcp``: la tarea y el repositorio de criterio ficticio por MCP
  (``universo-mcp --criterio``, sólo ``search`` y ``context``; ningún universo
  admite escritura).
- ``C_criterio_prompt``: la tarea y el texto de los criterios vigentes en el
  prompt.

Cada tarea es una decisión de código con dos o tres opciones. El agente
devuelve la opción, una justificación y el criterio del dueño que aplicó, o
``null``. El oráculo (``criterio_uso/oracle.json``) no se entrega al agente.

Las corridas reales usan Claude Code (`claude -p`) con la suscripción del
usuario y necesitan su OK explícito (E1 del plan de la fase 11);
``--agente guionizado`` recorre el mismo camino sin LLM.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
DATA = HERE / "criterio_uso"
RESULTS = HERE / "results"
PROTOCOLO = "felix-criterio-uso-v1"
BRAZOS = ["A_sin_criterio", "B_criterio_mcp", "C_criterio_prompt"]
REPETICIONES = 3
SEMILLA = 1119
TIMEOUT_S = 180
MCP_LLAMADAS = 6
MCP_HERRAMIENTAS = ["search", "context"]
MODELO = "claude-sonnet-5-5"
MUESTRAS_BOOTSTRAP = 10000
# Regla de decisión (docs/plan/11-evaluacion-criterio.md), fijada antes de correr.
MEJORA_MINIMA = 0.15
TOLERANCIA_MCP = 0.05
FUENTES = [
    "evaluation/criterio_uso/criterios.json", "evaluation/criterio_uso/tasks.json",
    "evaluation/criterio_uso/oracle.json", "evaluation/run_criterio_uso.py",
    "evaluation/mcp_proxy_v2.py", "evaluation/run_paired.py", "evaluation/run_four_arms.py",
    "evaluation/run_criterio_deteccion.py",
]
# Herramientas que el MCP anuncia y el brazo B no usa: se quitan del contexto.
MCP_EXCLUIDAS = ["propose", "check", "why"]

SISTEMA = ("Eres un agente de código que trabaja para el dueño de un proyecto. Para cada tarea eliges una de las "
           "opciones dadas, la que el dueño querría. Entrega la respuesta con la herramienta de salida estructurada.")
INSTRUCCION = ("Elige la opción para la tarea. Devuelve `opcion` (la letra), `justificacion` (una o dos frases) y "
               "`criterio_aplicado`: el texto del criterio del dueño que decidió tu elección, o null si ninguno "
               "aplica o no conoces criterios del dueño. No inspecciones archivos, no uses la red ni ejecutes "
               "comandos.")
NOTA_MCP = (f" El dueño guarda su criterio en un repositorio que puedes consultar con las herramientas `search` y "
            f"`context` (usa `include_values: true` y `universe: \"todos\"`; el universo `vigente` tiene criterio "
            f"confirmado y `candidatos`, criterio sin confirmar). `search` busca palabras en los títulos: usa pocas "
            f"palabras. Consúltalo antes de decidir, con un máximo de {MCP_LLAMADAS} llamadas.")
NOTA_PROMPT = " Este es el criterio vigente del dueño:"


class ErrorDeEntorno(RuntimeError):
    """El entorno o el proveedor no permiten seguir (sin bwrap, sin login, sin cuota)."""


def sha256_archivo(relativa):
    return hashlib.sha256((REPO / relativa).read_bytes()).hexdigest()


# --- Dataset ----------------------------------------------------------------------


def cargar(data=DATA):
    def leer(nombre):
        return json.loads((data / nombre).read_text(encoding="utf-8"))

    criterios, tareas, oraculo = leer("criterios.json"), leer("tasks.json"), leer("oracle.json")
    validar(criterios, tareas, oraculo)
    return criterios, tareas, oraculo


def validar(criterios, tareas, oraculo):
    vigentes = {c["id"] for c in criterios["vigentes"]}
    proyectos = set(criterios["proyectos"])
    for c in criterios["vigentes"]:
        if len(set(c["proyectos"])) < 2 or not set(c["proyectos"]) <= proyectos:
            raise ValueError(f"vigente_sin_dos_proyectos: {c['id']}")
    for c in criterios["candidatos"]:
        if c["proyecto"] not in proyectos:
            raise ValueError(f"candidato_sin_proyecto: {c['id']}")
    titulos = [c["titulo"].lower() for c in criterios["vigentes"] + criterios["candidatos"]]
    if len(set(titulos)) != len(titulos):
        raise ValueError("titulo_repetido")
    ids = [t["id"] for t in tareas["cases"]]
    if len(set(ids)) != len(ids) or set(ids) != set(oraculo["cases"]):
        raise ValueError("tareas_y_oraculo_no_coinciden")
    usados = []
    for t in tareas["cases"]:
        esperado = oraculo["cases"][t["id"]]
        if not 2 <= len(t["opciones"]) <= 3:
            raise ValueError(f"opciones_invalidas: {t['id']}")
        if esperado["criterio"] is None:
            if esperado["opcion"] is not None:
                raise ValueError(f"control_con_opcion: {t['id']}")
            continue
        if esperado["criterio"] not in vigentes:
            raise ValueError(f"criterio_desconocido: {t['id']}")
        if esperado["opcion"] not in t["opciones"]:
            raise ValueError(f"opcion_desconocida: {t['id']}")
        usados.append(esperado["criterio"])
    if len(set(usados)) != len(usados):
        raise ValueError("criterio_repetido_entre_tareas")


# --- Universo ficticio --------------------------------------------------------------


def _universo(binario, cwd, *args):
    # Ruta absoluta: los comandos corren con otro directorio de trabajo.
    proceso = subprocess.run([str(Path(binario).resolve()), *args], cwd=cwd, capture_output=True, text=True,
                             encoding="utf-8", timeout=120)
    if proceso.returncode != 0:
        raise RuntimeError(f"universo {args[0]} falló: {proceso.stdout}{proceso.stderr}")
    return proceso.stdout


def _nota(salida):
    for linea in salida.splitlines():
        if linea.startswith("note: "):
            return linea.split(": ", 1)[1].split()[0]
    raise RuntimeError(f"sin nota en: {salida}")


def construir_universo(criterios, destino, binario):
    """Repositorio de criterio ficticio en ``destino``: cada vigente capturado
    en un proyecto, reforzado en otro y promovido; las candidatas, sólo
    capturadas. Ningún universo admite escritura de agentes y no queda `.git`
    (el agente no lo necesita)."""
    destino = Path(destino)
    _universo(binario, destino.parent, "init", str(destino), "--git")
    comun = ["--autor", "fase-11", "--fecha", criterios["fecha"]]
    for c in criterios["vigentes"]:
        primero, segundo = c["proyectos"][:2]
        nota = _nota(_universo(binario, destino, "capturar", c["titulo"], "--proyecto", primero,
                               "--validacion", "explicita", *comun))
        _universo(binario, destino, "reforzar", nota, "--proyecto", segundo, "--contexto",
                  "se sostuvo en este proyecto", *comun)
        _universo(binario, destino, "promover", nota, "--a", "vigente", *comun)
    for c in criterios["candidatos"]:
        _universo(binario, destino, "capturar", c["titulo"], "--proyecto", c["proyecto"],
                  "--validacion", "implicita", *comun)
    toml = destino / "felix.toml"
    toml.write_text(toml.read_text(encoding="utf-8").replace("escritura_agentes = true",
                                                             "escritura_agentes = false"), encoding="utf-8")
    shutil.rmtree(destino / ".git", onerror=_quitar_solo_lectura)
    return destino


def _quitar_solo_lectura(funcion, ruta, _error):
    # En Windows los objetos de Git son de sólo lectura y rmtree no los borra.
    os.chmod(ruta, 0o700)
    funcion(ruta)


def contar(raiz, binario):
    resumen = json.loads(_universo(binario, raiz, "resumen", "--format=json"))
    estados = {u["name"]: u["criterios_by_state"] for u in resumen["universes"]}
    return {"vigente": estados["vigente"].get("vigente", 0),
            "candidatos": estados["candidatos"].get("candidato", 0),
            "promovidos": estados["candidatos"].get("promovido", 0)}


# --- Prompts ------------------------------------------------------------------------


def prompt(caso, brazo, criterios):
    texto = INSTRUCCION
    if brazo == "B_criterio_mcp":
        texto += NOTA_MCP
    elif brazo == "C_criterio_prompt":
        texto += NOTA_PROMPT + "\n" + "\n".join(f"- {c['titulo']}." for c in criterios["vigentes"])
    texto += "\n\nTarea: " + caso["escenario"] + "\nOpciones:\n"
    texto += "\n".join(f"{letra}) {caso['opciones'][letra]}" for letra in sorted(caso["opciones"]))
    return texto


def esquema(caso):
    return {"type": "object", "additionalProperties": False,
            "required": ["opcion", "justificacion", "criterio_aplicado"],
            "properties": {"opcion": {"type": "string", "enum": sorted(caso["opciones"])},
                           "justificacion": {"type": "string", "maxLength": 600},
                           "criterio_aplicado": {"type": ["string", "null"], "maxLength": 300}}}


# --- Puntaje ------------------------------------------------------------------------


def cita_criterio(respuesta):
    """En una tarea de control ningún criterio aplica: citar uno, exista o no
    en el repositorio, es aplicar criterio donde no corresponde."""
    cita = (respuesta or {}).get("criterio_aplicado")
    return isinstance(cita, str) and bool(cita.strip())


def _valida(f):
    return f["status"] == "completed" and isinstance(f.get("respuesta"), dict)


def puntuar(filas, oraculo):
    """Por brazo: aciertos de adherencia sobre las tareas con criterio, criterio
    inventado sobre las de control y filas sin respuesta, que no cuentan como
    acierto ni como error: se informan aparte."""
    salida = {}
    for brazo in BRAZOS:
        r = {"adherencia_aciertos": 0, "adherencia_total": 0, "inventado": 0, "control_total": 0,
             "sin_respuesta": 0, "tool_calls": 0, "segundos": 0.0, "filas": 0}
        for f in (f for f in filas if f["arm"] == brazo):
            r["filas"] += 1
            r["tool_calls"] += len(f.get("tool_calls") or [])
            r["herramientas_no_permitidas"] = r.get("herramientas_no_permitidas", 0) + len(
                f.get("herramientas_no_permitidas") or [])
            r["segundos"] += f.get("elapsed_seconds") or 0.0
            if not _valida(f):
                r["sin_respuesta"] += 1
                continue
            esperado = oraculo["cases"][f["case"]]
            if esperado["criterio"]:
                r["adherencia_total"] += 1
                r["adherencia_aciertos"] += f["respuesta"].get("opcion") == esperado["opcion"]
            else:
                r["control_total"] += 1
                r["inventado"] += cita_criterio(f["respuesta"])
        total = r["adherencia_total"]
        r["adherencia"] = round(r["adherencia_aciertos"] / total, 4) if total else None
        r["inventado_tasa"] = round(r["inventado"] / r["control_total"], 4) if r["control_total"] else None
        r["segundos"] = round(r["segundos"], 2)
        salida[brazo] = r
    return salida


def _por_tarea(filas, oraculo, brazo):
    aciertos = {}
    for f in filas:
        esperado = oraculo["cases"][f["case"]]
        if f["arm"] != brazo or not _valida(f) or not esperado["criterio"]:
            continue
        aciertos.setdefault(f["case"], []).append(f["respuesta"].get("opcion") == esperado["opcion"])
    return {t: sum(v) / len(v) for t, v in aciertos.items()}


def diferencia(filas, oraculo, brazo, contra, semilla=SEMILLA, muestras=MUESTRAS_BOOTSTRAP):
    """Diferencia de adherencia ``brazo - contra``, pareada por tarea, con un
    intervalo bootstrap al 95 % que remuestrea tareas."""
    x, y = _por_tarea(filas, oraculo, brazo), _por_tarea(filas, oraculo, contra)
    tareas = sorted(set(x) & set(y))
    if not tareas:
        return {"diferencia": None, "ic95": None, "tareas": 0}
    difs = [x[t] - y[t] for t in tareas]
    rng = random.Random(semilla)
    medias = sorted(sum(rng.choice(difs) for _ in difs) / len(difs) for _ in range(muestras))
    bajo, alto = medias[int(0.025 * (muestras - 1))], medias[int(0.975 * (muestras - 1))]
    return {"diferencia": round(sum(difs) / len(difs), 4), "ic95": [round(bajo, 4), round(alto, 4)],
            "tareas": len(tareas)}


def decidir(resumen):
    """Regla de decisión del plan. Las diferencias son pareadas por tarea (sólo
    tareas con respuesta en ambos brazos); el criterio inventado se compara
    como tasa sobre las tareas de control respondidas."""
    a = resumen["brazos"]["A_sin_criterio"]
    b = resumen["brazos"]["B_criterio_mcp"]
    dif, contra_c = resumen["B_menos_A"], resumen["B_menos_C"]
    tasa = lambda r: r.get("inventado_tasa") or 0.0  # noqa: E731
    ayuda = (dif["diferencia"] is not None and dif["diferencia"] >= MEJORA_MINIMA and dif["ic95"][0] > 0
             and tasa(b) <= tasa(a))
    aporta = contra_c["diferencia"] is not None and contra_c["diferencia"] >= -TOLERANCIA_MCP
    return {"criterio_ayuda": bool(ayuda), "mcp_aporta": bool(aporta),
            "regla": f"ayuda: B-A pareada >= {MEJORA_MINIMA}, IC95 excluye 0 y la tasa de criterio citado en "
                     f"control de B no supera la de A; MCP aporta: B-C pareada >= -{TOLERANCIA_MCP}"}


def informe(filas, oraculo, semilla=SEMILLA, muestras=MUESTRAS_BOOTSTRAP):
    resumen = {"brazos": puntuar(filas, oraculo)}
    for nombre, x, y in (("B_menos_A", "B_criterio_mcp", "A_sin_criterio"),
                         ("C_menos_A", "C_criterio_prompt", "A_sin_criterio"),
                         ("B_menos_C", "B_criterio_mcp", "C_criterio_prompt")):
        resumen[nombre] = diferencia(filas, oraculo, x, y, semilla, muestras)
    resumen["decision"] = decidir(resumen)
    return resumen


# --- Plan ---------------------------------------------------------------------------


def construir_plan(modelo, semilla=SEMILLA, repeticiones=REPETICIONES):
    """Cada (tarea, repetición) corre los tres brazos en un orden aleatorio propio."""
    _, tareas, _ = cargar()
    rng = random.Random(semilla)
    filas = []
    for repeticion in range(1, repeticiones + 1):
        casos = sorted(t["id"] for t in tareas["cases"])
        rng.shuffle(casos)
        for caso in casos:
            brazos = list(BRAZOS)
            rng.shuffle(brazos)
            filas.extend({"case": caso, "repetition": repeticion, "arm": b} for b in brazos)
    return {"schema": PROTOCOLO, "configured_model_alias": modelo, "schedule_seed": semilla,
            "repetitions": repeticiones, "arms": BRAZOS, "timeout_seconds": TIMEOUT_S,
            "mcp_tool_call_budget_b": MCP_LLAMADAS, "mcp_tools_b": MCP_HERRAMIENTAS,
            "system_prompt": SISTEMA, "instruction": INSTRUCCION, "mcp_note": NOTA_MCP, "prompt_note": NOTA_PROMPT,
            "decision_rule": {"mejora_minima": MEJORA_MINIMA, "tolerancia_mcp": TOLERANCIA_MCP,
                              "bootstrap": {"muestras": MUESTRAS_BOOTSTRAP, "semilla": SEMILLA, "unidad": "tarea"}},
            "primary_endpoints": ["adherencia (oráculo)", "criterio inventado en control"],
            "secondary_endpoints": ["sin_respuesta", "tool_calls", "elapsed_seconds", "provider_usage"],
            "stopping_rule": "correr todas las filas del plan y publicar todas; sin ampliar ni repetir corridas",
            "source_sha256": {f: sha256_archivo(f) for f in FUENTES},
            "runs": len(filas), "schedule": filas}


# --- Agentes ------------------------------------------------------------------------


class AgenteGuionizado:
    """Respuestas deterministas: ``politica(caso, brazo) -> respuesta``. Sin LLM."""

    def __init__(self, politica):
        self.politica = politica
        self.llamadas = []

    def responder(self, caso, brazo, texto, universo):
        self.llamadas.append({"case": caso["id"], "arm": brazo, "prompt": texto})
        return {"status": "completed", "respuesta": self.politica(caso, brazo), "tool_calls": [],
                "elapsed_seconds": 0.0, "usage": []}


class AgenteClaude:
    """Claude Code (`claude -p`) aislado con bubblewrap, como en U13 y los
    cuatro brazos: HOME temporal, sin ajustes del usuario ni herramientas
    integradas. En el brazo B, sólo `search` y `context` del MCP de Felix por el
    proxy v2, sobre una copia del repositorio ficticio montada en sólo lectura."""

    def __init__(self, claude_binary, mcp_binary, modelo, timeout=TIMEOUT_S):
        from run_criterio_deteccion import binario_real

        self.claude_binary = binario_real(claude_binary)
        self.mcp_binary = Path(mcp_binary).resolve()
        self.modelo = modelo
        self.timeout = timeout

    @staticmethod
    def credenciales():
        return Path.home() / ".claude/.credentials.json"

    def command(self, trabajo, logs, home, caso, con_mcp):
        cmd = ["bwrap", "--unshare-all", "--share-net", "--die-with-parent", "--new-session", "--clearenv"]
        for path in ["/usr", "/bin", "/lib", "/lib64", "/etc/ssl", "/etc/ca-certificates", "/etc/resolv.conf",
                     "/etc/hosts", "/etc/nsswitch.conf"]:
            if Path(path).exists():
                cmd += ["--ro-bind", path, path]
        cmd += ["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp",
                "--bind", str(home), "/agente-home",
                "--bind", str(self.credenciales()), "/agente-home/.claude/.credentials.json",
                "--ro-bind", str(self.claude_binary), "/opt/claude/claude",
                "--ro-bind", str(trabajo), "/work", "--bind", str(logs), "/logs", "--chdir", "/tmp",
                "--setenv", "HOME", "/agente-home", "--setenv", "PATH", "/usr/bin:/bin", "--setenv", "LANG", "C.UTF-8"]
        if con_mcp:
            cmd += ["--ro-bind", str(self.mcp_binary), "/tools/universo-mcp",
                    "--ro-bind", str(HERE / "mcp_proxy_v2.py"), "/tools/mcp_proxy_v2.py"]
        cmd += ["/opt/claude/claude", "-p", "--output-format", "stream-json", "--verbose",
                "--model", self.modelo, "--tools", "", "--setting-sources", "", "--strict-mcp-config",
                "--no-session-persistence", "--max-turns", str(MCP_LLAMADAS + 4),
                "--system-prompt", SISTEMA, "--json-schema", json.dumps(esquema(caso))]
        if con_mcp:
            servidor = {"mcpServers": {"felix": {"command": "/usr/bin/python3", "args": [
                "/tools/mcp_proxy_v2.py", "/logs/tools.jsonl", str(MCP_LLAMADAS),
                "/tools/universo-mcp", "--criterio", "/work/criterio"]}}}
            cmd += ["--mcp-config", json.dumps(servidor),
                    "--allowedTools", ",".join(f"mcp__felix__{t}" for t in MCP_HERRAMIENTAS),
                    "--disallowedTools", ",".join(f"mcp__felix__{t}" for t in MCP_EXCLUIDAS)]
        return cmd

    def preflight(self):
        if os.name == "nt" or shutil.which("bwrap") is None:
            raise ErrorDeEntorno("bwrap_required")
        if not self.credenciales().is_file():
            raise ErrorDeEntorno("claude_login_required")
        for ruta in (self.claude_binary, self.mcp_binary):
            if not ruta.is_file():
                raise ErrorDeEntorno(f"missing_{ruta.name}")

    def version(self):
        return subprocess.check_output([str(self.claude_binary), "--version"], text=True,
                                       stderr=subprocess.DEVNULL).strip()

    def responder(self, caso, brazo, texto, universo):
        from run_paired import bounded_capture, parse_events
        from run_four_arms import fallo_del_proveedor, interpretar_claude

        con_mcp = brazo == "B_criterio_mcp"
        with tempfile.TemporaryDirectory(prefix="felix-criterio-uso-") as temporal:
            raiz = Path(temporal)
            trabajo, logs, home = raiz / "work", raiz / "logs", raiz / "home"
            trabajo.mkdir()
            logs.mkdir()
            (home / ".claude").mkdir(parents=True)
            if con_mcp:
                shutil.copytree(universo, trabajo / "criterio")
            inicio = time.monotonic()
            try:
                proceso = subprocess.Popen(self.command(trabajo, logs, home, caso, con_mcp), stdin=subprocess.PIPE,
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
                                           env={"PATH": "/usr/bin:/bin"})
            except OSError as error:
                raise ErrorDeEntorno(f"spawn_failed_{type(error).__name__}") from error
            stdout, _stderr, timed_out, overflow = bounded_capture(proceso, texto.encode("utf-8"), self.timeout)
            segundos = time.monotonic() - inicio
            eventos = parse_events(stdout.decode(errors="replace"))
            proveedor = fallo_del_proveedor(eventos)
            if proveedor is not None:
                raise ErrorDeEntorno(proveedor)
            respuesta, usos = interpretar_claude(eventos)
            ledger = logs / "tools.jsonl"
            tools = [json.loads(linea) for linea in ledger.read_text().splitlines()] if ledger.exists() else []
            permitidas = {"StructuredOutput"} | ({f"mcp__felix__{t}" for t in MCP_HERRAMIENTAS} if con_mcp else set())
            # Un intento de usar otra herramienta (denegado por permisos) no anula
            # la fila: se informa aparte, para no penalizar sólo al brazo B.
            inesperadas = [n for n in usos if n not in permitidas]
            estado = ("timeout" if timed_out else "output_limit" if overflow else
                      "completed" if proceso.returncode == 0 and respuesta is not None else "execution_failed")
            return {"status": estado, "respuesta": respuesta, "tool_calls": tools, "elapsed_seconds": segundos,
                    "herramientas_no_permitidas": inesperadas,
                    "usage": [e.get("usage") for e in eventos if e.get("type") == "result" and "usage" in e]}


# --- Corrida ------------------------------------------------------------------------


def correr(plan, agente, salida, universo):
    """Corre las filas del plan que faltan en ``salida`` (JSONL, una fila por
    línea). Un corte del entorno o del proveedor detiene la corrida y conserva
    lo medido; devuelve su código, o None si terminó."""
    criterios, tareas, _ = cargar()
    casos = {t["id"]: t for t in tareas["cases"]}
    salida = Path(salida)
    huella = huella_plan(plan)
    hechas = set()
    previas = leer_filas(salida)
    for f in previas:
        if f.get("plan_sha256") != huella:
            return "resume_plan_mismatch"
        hechas.add((f["case"], f["repetition"], f["arm"]))
    if salida.exists() and len(salida.read_text(encoding="utf-8").splitlines()) != len(previas):
        # Se descartó una última línea truncada: se reescribe sin ella antes de
        # agregar filas, para que no quede en medio del archivo.
        salida.write_text("".join(json.dumps(f, ensure_ascii=False) + "\n" for f in previas), encoding="utf-8")
    for fila in plan["schedule"]:
        if (fila["case"], fila["repetition"], fila["arm"]) in hechas:
            continue
        caso = casos[fila["case"]]
        try:
            r = agente.responder(caso, fila["arm"], prompt(caso, fila["arm"], criterios), universo)
        except ErrorDeEntorno as error:
            return str(error)
        with salida.open("a", encoding="utf-8") as archivo:
            archivo.write(json.dumps({**fila, **r, "plan_sha256": huella}, ensure_ascii=False) + "\n")
    return None


def huella_plan(plan):
    return hashlib.sha256(json.dumps(plan, sort_keys=True, ensure_ascii=False).encode("utf-8")).hexdigest()


def leer_filas(salida):
    """Filas ya medidas. Una última línea truncada (corte a mitad de escritura)
    se descarta: esa fila se vuelve a correr. Una línea inválida en otro lugar
    es un error."""
    salida = Path(salida)
    if not salida.exists():
        return []
    lineas = salida.read_text(encoding="utf-8").splitlines()
    filas = []
    for i, linea in enumerate(lineas):
        try:
            filas.append(json.loads(linea))
        except ValueError:
            if i == len(lineas) - 1:
                break
            raise
    return filas


def _commit():
    try:
        return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True,
                                       stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--agente", choices=["claude", "guionizado"], required=True)
    parser.add_argument("--model", default=MODELO)
    parser.add_argument("--claude-binary", default=shutil.which("claude") or "claude")
    exe = ".exe" if os.name == "nt" else ""
    parser.add_argument("--binary", default=str(REPO / "target/debug" / f"universo{exe}"))
    parser.add_argument("--mcp-binary", default=str(REPO / "target/debug" / f"universo-mcp{exe}"))
    parser.add_argument("--plan-only", action="store_true", help="escribe el plan y termina")
    parser.add_argument("--salida", type=Path, help="directorio de resultados")
    args = parser.parse_args(argv)

    plan = construir_plan(args.model)
    if args.plan_only:
        (DATA / "plan.json").write_text(json.dumps(plan, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"plan: {plan['runs']} corridas -> {DATA / 'plan.json'}")
        return 0
    publicado = json.loads((DATA / "plan.json").read_text(encoding="utf-8"))
    if args.agente == "claude" and publicado != plan:
        print("el plan publicado no coincide con las fuentes actuales: no se corre", file=sys.stderr)
        return 2
    destino = args.salida or RESULTS / f"criterio-uso-{args.agente}"
    destino.mkdir(parents=True, exist_ok=True)
    criterios, _, oraculo = cargar()
    if args.agente == "guionizado":
        agente = AgenteGuionizado(lambda caso, brazo: {"opcion": sorted(caso["opciones"])[0],
                                                        "justificacion": "", "criterio_aplicado": None})
    else:
        try:
            agente = AgenteClaude(args.claude_binary, args.mcp_binary, args.model)
            agente.preflight()
        except Exception as error:  # noqa: BLE001 - cualquier fallo del entorno corta antes de correr
            print(f"error de entorno: {error}", file=sys.stderr)
            return 2
    with tempfile.TemporaryDirectory(prefix="felix-criterio-universo-") as temporal:
        universo = construir_universo(criterios, Path(temporal) / "criterio", Path(args.binary))
        corte = correr(plan, agente, destino / "runs.jsonl", universo)
    filas = leer_filas(destino / "runs.jsonl")
    resumen = informe(filas, oraculo)
    resumen.update({"protocolo": PROTOCOLO, "modelo": args.model, "filas": len(filas), "plan_runs": plan["runs"],
                    "plan_sha256": huella_plan(plan), "git_commit": _commit(),
                    "binarios": {"universo": _universo(args.binary, REPO, "--version").strip()},
                    "corte": corte, "completo": corte is None and len(filas) == plan["runs"]})
    (destino / "summary.json").write_text(json.dumps(resumen, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    for brazo, r in resumen["brazos"].items():
        print(f"{brazo}: adherencia {r['adherencia']} ({r['adherencia_aciertos']}/{r['adherencia_total']}), "
              f"inventado {r['inventado']}/{r['control_total']}, sin respuesta {r['sin_respuesta']}")
    print(f"B-A {resumen['B_menos_A']} -> decisión {resumen['decision']}")
    if corte:
        print(f"corrida cortada: {corte}; {len(filas)} filas", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
