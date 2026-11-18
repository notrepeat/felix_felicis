#!/usr/bin/env python3
"""Runner del estudio prerregistrado de cuatro brazos (propuesta 09, protocolo felix-four-arms-v1).

Brazos, con el mismo modelo, información y presupuesto por ronda:

- ``A_baseline``: una respuesta, sin Felix.
- ``B_felix_opcional``: una respuesta, con las herramientas MCP de lectura.
- ``C_obligatorio``: cada delta pasa por el orquestador del paso 8; el rechazo
  vuelve redactado y hay hasta ``RONDAS`` intentos. El resultado final es lo
  publicado, o ningún cambio si se escala.
- ``D_revision``: hasta ``RONDAS`` rondas con una instrucción neutral de
  revisión que no dice si la propuesta es válida; termina cuando el agente
  repite su delta.

El runner del piloto 173 (``run_paired.py``) no se modifica; de él sólo se
reutilizan funciones de captura acotada y parseo de eventos.

Las corridas reales necesitan una instalación autenticada de Codex CLI y una
decisión explícita (fases 9b y 9c de la propuesta); ``--agente guionizado``
recorre el mismo camino con respuestas deterministas, sin LLM.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
from collections import Counter
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
sys.path.insert(0, str(REPO / "tools"))

import four_arms_scoring as scoring  # noqa: E402
import orquestador as orq  # noqa: E402

PROTOCOL = "felix-four-arms-v1"
ARMS = ["A_baseline", "B_felix_opcional", "C_obligatorio", "D_revision"]
RONDAS = 3
REPETICIONES = 3
SEMILLA = 914
TIMEOUT_RONDA_S = 120
MCP_LLAMADAS = 8
MAX_CORRIDAS = 96
DATA = HERE / "four_arms"
PLAN = DATA / "plan.json"
SOURCES = [
    "evaluation/four_arms/tasks.json", "evaluation/four_arms/oracle.json", "evaluation/four_arms_dataset.py",
    "evaluation/four_arms_scoring.py", "evaluation/run_four_arms.py", "evaluation/mcp_proxy_v2.py",
    "evaluation/run_paired.py", "tools/orquestador.py",
]
RESULTS = HERE / "results"
MCP_TOOLS = ["check", "context", "propose", "why"]
# Resultados del orquestador que no son un fallo técnico: publicó, o el agente
# agotó sus intentos con propuestas rechazadas.
C_SIN_FALLO_TECNICO = {("publicado", None), ("escalado", "presupuesto_agotado")}
SCHEMA = {"type": "object", "additionalProperties": False, "required": ["schema", "add", "remove"],
          "properties": {"schema": {"type": "string", "enum": ["felix-delta-v1"]},
                         "add": {"type": "array", "items": {"type": "string"}},
                         "remove": {"type": "array", "items": {"type": "string"}}}}
INSTRUCCION = ("Resuelve la tarea sintética de edición de un grafo de conocimiento. Devuelve sólo el delta JSON "
               "felix-delta-v1 pedido. Usa sólo los hechos y reglas dados. No inspecciones archivos, no uses la red "
               "ni ejecutes comandos. Minimiza los cambios ajenos al objetivo. Un delta vacío (add y remove vacíos) "
               "significa no cambiar nada.")
NOTA_MCP = (f" Tienes herramientas de verificación de Felix (context, check, propose, why); puedes usarlas para "
            f"comprobar y reparar tu propuesta, con un máximo de {MCP_LLAMADAS} llamadas.")
REVISION_NEUTRAL = ("Revisa tu propuesta anterior contra las reglas y el objetivo. Devuelve el delta final; "
                    "si ya es correcto, devuélvelo sin cambios.")
NULO = {"schema": "felix-delta-v1", "add": [], "remove": []}


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False).encode("utf-8")).hexdigest()


def file_sha256(relative):
    return hashlib.sha256((REPO / relative).read_bytes()).hexdigest()


def schedule(cases, seed=SEMILLA, repetitions=REPETICIONES):
    """Cada (tarea, repetición) corre los cuatro brazos en un orden aleatorio propio."""
    rng = random.Random(seed)
    rows = []
    for repetition in range(repetitions):
        shuffled = sorted(cases, key=lambda c: c["id"])
        rng.shuffle(shuffled)
        for case in shuffled:
            arms = list(ARMS)
            rng.shuffle(arms)
            rows.extend((case["id"], repetition + 1, arm) for arm in arms)
    return rows


def build_plan(model, seed=SEMILLA, repetitions=REPETICIONES):
    tasks, _ = scoring.load()
    rows = schedule(tasks["cases"], seed, repetitions)
    return {
        "schema": PROTOCOL, "configured_model_alias": model, "provider_model_snapshot": None, "sampling_seed": None,
        "schedule_seed": seed, "repetitions": repetitions, "arms": ARMS, "rounds_c_d": RONDAS,
        "timeout_seconds_per_round": TIMEOUT_RONDA_S, "mcp_tool_call_budget_b": MCP_LLAMADAS,
        "cases": [c["id"] for c in tasks["cases"]], "runs": len(rows),
        "source_sha256": {name: file_sha256(name) for name in SOURCES},
        "instruction": INSTRUCCION, "mcp_note": NOTA_MCP, "neutral_review": REVISION_NEUTRAL,
        "schema_sha256": digest(SCHEMA),
        "primary_endpoints": ["final_invalid_state (oráculo)", "final_success (oráculo)"],
        "secondary_endpoints": ["first_delta_success", "repaired", "rounds", "escalated", "elapsed_seconds",
                                "provider_usage"],
        "stopping_rule": "correr todas las filas del plan y publicar todas; sin ampliar ni repetir corridas",
        "schedule": [{"case": c, "repetition": r, "arm": a} for c, r, a in rows],
    }


def prompt(case, arm, historia=()):
    texto = INSTRUCCION + (NOTA_MCP if arm == "B_felix_opcional" else "")
    texto += "\n" + json.dumps({k: case[k] for k in ["prompt", "facts", "rules", "invariants"]}, ensure_ascii=False)
    for entrada in historia:
        texto += "\n" + entrada
    return texto


def materialize(case, root):
    for relative, content in case["files"].items():
        path = Path(relative)
        if path.is_absolute() or ".." in path.parts:
            raise ValueError("unsafe_fixture_path")
        target = root / "bundle" / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content.encode("utf-8"))
    (root / "manifest.toml").write_bytes(case["manifest"].encode("utf-8"))
    (root / "invariants.sx").write_bytes(case["invariants"].encode("utf-8"))
    (root / "schema.json").write_text(json.dumps(SCHEMA), encoding="utf-8")
    return orq.Repositorio(root / "bundle", root / "manifest.toml", root / "invariants.sx")


class ErrorDeEntorno(RuntimeError):
    """El entorno no permite correr (sin bwrap, sin login, sin Codex): aborta sin consumir la fila."""


def fallo_del_proveedor(events):
    """Código de un fallo del proveedor (cuota, límite de uso, turno fallido), o None.

    Es un error de entorno, no una respuesta del agente: el modelo no llegó a
    contestar. Los avisos de la CLI (eventos `item` de tipo `error`) no cuentan.
    """
    for event in events:
        if event.get("type") not in ("error", "turn.failed"):
            continue
        mensaje = event.get("message") or (event.get("error") or {}).get("message") or ""
        texto = str(mensaje).lower()
        if "usage limit" in texto or "rate limit" in texto or "quota" in texto:
            return "provider_usage_limit"
        return "provider_turn_failed"
    return None


# --- Agentes ------------------------------------------------------------------------


class AgenteGuionizado:
    """Respuestas deterministas: ``politica(case_id, ronda, prompt) -> delta``. Sin LLM."""

    def __init__(self, politica):
        self.politica = politica
        self.llamadas = []

    def responder(self, case, texto, con_mcp, ronda):
        self.llamadas.append({"case": case["id"], "ronda": ronda, "con_mcp": con_mcp, "prompt": texto})
        delta = self.politica(case["id"], ronda, texto)
        return {"status": "completed", "delta": delta, "usage": [], "elapsed_seconds": 0.0, "tool_calls": []}


class AgenteCodex:
    """Codex CLI aislado con bubblewrap, como en el piloto; MCP vía proxy v2 en el brazo B."""

    DISABLED = ["shell_tool", "apps", "browser_use", "multi_agent", "multi_agent_v2", "memories", "plugins",
                "remote_plugin", "skill_search", "view_image", "image_generation"]

    def __init__(self, codex_dir, mcp_binary, model, timeout=TIMEOUT_RONDA_S):
        self.codex_dir = Path(codex_dir).resolve()
        self.mcp_binary = Path(mcp_binary).resolve()
        self.model = model
        self.timeout = timeout

    def command(self, work, logs, con_mcp):
        home = Path.home()
        auth = home / ".codex/auth.json"
        if not auth.is_file():
            raise ErrorDeEntorno("existing_codex_file_login_required")
        cmd = ["bwrap", "--unshare-all", "--share-net", "--die-with-parent", "--new-session", "--clearenv"]
        for path in ["/usr", "/bin", "/lib", "/lib64", "/etc/ssl", "/etc/ca-certificates", "/etc/resolv.conf",
                     "/etc/hosts", "/etc/nsswitch.conf"]:
            if Path(path).exists():
                cmd += ["--ro-bind", path, path]
        cmd += ["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp", "--dir", str(home / ".codex"),
                "--ro-bind", str(auth), str(auth), "--ro-bind", str(self.codex_dir), "/opt/codex",
                "--ro-bind", str(self.mcp_binary), "/tools/universo-mcp",
                "--ro-bind", str(HERE / "mcp_proxy_v2.py"), "/tools/mcp_proxy_v2.py",
                "--ro-bind", str(work), "/work", "--bind", str(logs), "/logs", "--chdir", "/work",
                "--setenv", "HOME", str(home), "--setenv", "PATH", "/usr/bin:/bin", "--setenv", "LANG", "C.UTF-8",
                "/opt/codex/bin/codex", "exec", "--ephemeral", "--ignore-user-config", "--skip-git-repo-check",
                "--sandbox", "read-only", "--model", self.model, "-c", 'model_reasoning_effort="low"',
                "-c", 'web_search="disabled"', "-c", "project_doc_max_bytes=0",
                "--enable", "skip_host_skill_discovery", "--output-schema", "/work/schema.json", "--json"]
        for feature in self.DISABLED:
            cmd += ["--disable", feature]
        if con_mcp:
            cmd += ["-c", 'mcp_servers.felix.command="/usr/bin/python3"',
                    "-c", f'mcp_servers.felix.args=["/tools/mcp_proxy_v2.py","/logs/tools.jsonl","{MCP_LLAMADAS}"]',
                    "-c", "mcp_servers.felix.startup_timeout_sec=10", "-c", "mcp_servers.felix.tool_timeout_sec=10"]
        return cmd + ["-"]

    def preflight(self):
        """Comprueba el entorno antes de la primera fila, sin llamar al modelo."""
        if os.name == "nt" or shutil.which("bwrap") is None:
            raise ErrorDeEntorno("bwrap_required")
        if not (Path.home() / ".codex/auth.json").is_file():
            raise ErrorDeEntorno("existing_codex_file_login_required")
        for ruta in (self.codex_dir / "bin/codex", self.mcp_binary):
            if not ruta.is_file():
                raise ErrorDeEntorno(f"missing_{ruta.name}")

    def responder(self, case, texto, con_mcp, ronda):
        from run_paired import bounded_capture, parse_events

        with tempfile.TemporaryDirectory(prefix="felix-four-arms-") as temporary:
            root = Path(temporary)
            work, logs = root / "work", root / "logs"
            work.mkdir()
            logs.mkdir()
            materialize(case, work)
            start = time.monotonic()
            try:
                process = subprocess.Popen(self.command(work, logs, con_mcp), stdin=subprocess.PIPE,
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
                                           env={"PATH": "/usr/bin:/bin"})
            except OSError as error:
                raise ErrorDeEntorno(f"spawn_failed_{type(error).__name__}") from error
            stdout, _stderr, timed_out, overflow = bounded_capture(process, texto.encode("utf-8"), self.timeout)
            elapsed = time.monotonic() - start
            events = parse_events(stdout.decode(errors="replace"))
            proveedor = fallo_del_proveedor(events)
            if proveedor is not None:
                # Se aborta sin escribir la fila: al recuperar la cuota,
                # `--resume` la corre.
                raise ErrorDeEntorno(proveedor)
            messages = [e["item"]["text"] for e in events if e.get("type") == "item.completed"
                        and e.get("item", {}).get("type") == "agent_message"]
            delta = None
            if messages:
                try:
                    delta = json.loads(messages[-1])
                except ValueError:
                    delta = None
            ledger = logs / "tools.jsonl"
            tools = [json.loads(line) for line in ledger.read_text().splitlines()] if ledger.exists() else []
            items = [e.get("item", {}) for e in events if e.get("type") in ["item.completed", "item.started"]]
            # Como en el piloto: sólo se admiten mensajes, razonamiento y, en B,
            # las cuatro herramientas de Felix.
            unexpected = [item for item in items if item.get("type") not in ["agent_message", "reasoning", "error"]
                          and not (con_mcp and item.get("type") == "mcp_tool_call" and item.get("server") == "felix"
                                   and item.get("tool") in MCP_TOOLS)]
            ready = None
            if con_mcp:
                try:
                    ready = json.loads((logs / "tools.jsonl.ready").read_text()) == {"tools": MCP_TOOLS}
                except (OSError, ValueError):
                    ready = False
            status = ("timeout" if timed_out else "output_limit" if overflow else
                      "unexpected_tool" if unexpected else "mcp_unavailable" if ready is False else
                      "completed" if process.returncode == 0 and delta is not None else "execution_failed")
            return {"status": status, "delta": delta, "elapsed_seconds": elapsed, "tool_calls": tools,
                    "mcp_ready": ready,
                    "usage": [e["usage"] for e in events if e.get("type") == "turn.completed" and "usage" in e]}


# --- Brazos ---------------------------------------------------------------------------


def _historia_delta(delta):
    return "Tu propuesta anterior fue: " + json.dumps(delta, ensure_ascii=False)


def _historia_felix(retro):
    partes = [f"{o.get('rule_id')}: {o.get('rule_message')}" for o in retro.get("objetos") or []]
    partes += [f"error de formato del delta: {c}" for c in retro.get("codigos") or []]
    return (f"Felix rechazó esa propuesta ({'; '.join(partes) or 'sin detalle'}). "
            f"Intentos restantes: {retro['intentos_restantes']}. Devuelve un delta corregido.")


def _llamar(agente, case, texto, con_mcp, ronda):
    respuesta = dict(agente.responder(case, texto, con_mcp, ronda))
    # Se publica el hash del prompt efectivo; el texto se reconstruye desde el
    # plan, la tarea y las rondas previas.
    respuesta["prompt_sha256"] = hashlib.sha256(texto.encode("utf-8")).hexdigest()
    # Una ronda sin un objeto delta es un fallo técnico en todos los brazos.
    if respuesta["status"] == "completed" and not isinstance(respuesta["delta"], dict):
        respuesta["status"] = "sin_delta"
    return respuesta


def run_arm(case, arm, agente, binario, trabajo):
    """Ejecuta un brazo y devuelve (rondas, delta_final, extra) sin puntuar."""
    rondas = []
    if arm in ("A_baseline", "B_felix_opcional"):
        rondas.append(_llamar(agente, case, prompt(case, arm), arm == "B_felix_opcional", 1))
        return rondas, rondas[0]["delta"], {}
    if arm == "D_revision":
        # El historial se acumula igual que en C: cada propuesta previa, cada
        # una seguida de la misma instrucción neutral.
        historia = []
        for ronda in range(1, RONDAS + 1):
            respuesta = _llamar(agente, case, prompt(case, arm, historia), False, ronda)
            rondas.append(respuesta)
            if respuesta["status"] != "completed":
                break
            if ronda > 1 and respuesta["delta"] == rondas[-2]["delta"]:
                break
            historia += [_historia_delta(respuesta["delta"]), REVISION_NEUTRAL]
        return rondas, rondas[-1]["delta"], {}
    # C_obligatorio: el orquestador decide; el agente sólo propone.
    repo = materialize(case, trabajo / "repo")
    sello = subprocess.run([str(binario), "seal", str(repo.raiz), f"--manifiesto={repo.manifiesto}",
                            f"--invariantes={repo.invariantes}", "--format=json"], capture_output=True, timeout=60)
    if sello.returncode != 0:
        raise RuntimeError("seal_failed")
    historia = []

    class Adaptador:
        def proponer(self, vista, retroalimentacion):
            if retroalimentacion is not None:
                historia.extend([_historia_delta(rondas[-1]["delta"]), _historia_felix(retroalimentacion)])
            respuesta = _llamar(agente, case, prompt(case, arm, historia), False, len(rondas) + 1)
            rondas.append(respuesta)
            return respuesta["delta"] if respuesta["status"] == "completed" else None

    resultado = orq.Orquestador(binario, repo, presupuesto=RONDAS, reparar_errores_de_delta=True).ejecutar(
        Adaptador(), f"tarea {case['id']}")
    final = rondas[-1]["delta"] if resultado["resultado"] == "publicado" else NULO
    return rondas, final, {"orquestador": resultado}


def execute(case, repetition, arm, agente, binario, contexto=None):
    """Una fila del plan. Regla única de puntuación, prerregistrada:

    - Fallo técnico (`technical_failure`): alguna ronda no se completó (timeout,
      sin delta, MCP no disponible, herramienta inesperada...), el runner falló,
      o en C el orquestador terminó por algo distinto de publicar o de agotar
      los intentos (incluido `incierto`). Esas filas se publican y se cuentan por
      brazo, pero quedan fuera de los endpoints primarios: `score` y
      `final_invalid` son `null`.
    - En el resto, el oráculo puntúa el delta final de cada brazo.
    """
    record = {"case_id": case["id"], "family": case["family"], "repetition": repetition, "arm": arm,
              **(contexto or {})}
    entorno = []

    class Vigilado:
        # El orquestador convierte cualquier excepción del agente en
        # `error_agente`; un error de entorno tiene que salir del runner.
        def responder(self, *argumentos):
            try:
                return agente.responder(*argumentos)
            except ErrorDeEntorno as error:
                entorno.append(error)
                raise

    try:
        with tempfile.TemporaryDirectory(prefix="felix-four-arms-run-") as temporary:
            rondas, final, extra = run_arm(case, arm, Vigilado(), Path(binario), Path(temporary))
    except ErrorDeEntorno:
        raise
    except Exception as error:  # noqa: BLE001 - una fila que falla se registra, no se repite
        return {**record, "status": "runner_error", "error": type(error).__name__, "technical_failure": True,
                "rounds": None, "first_delta": None, "first_score": None, "final_delta": None, "score": None,
                "final_invalid": None, "repaired": None, "elapsed_seconds": None, "usage": [], "tool_calls": [],
                "prompt_sha256": []}
    if entorno:
        raise entorno[0]
    fallos = [r["status"] for r in rondas if r["status"] != "completed"]
    if not rondas:
        fallos = ["sin_rondas"]
    orquestador = extra.get("orquestador")
    tecnico = bool(fallos) or (orquestador is not None
                               and (orquestador["resultado"], orquestador["motivo"]) not in C_SIN_FALLO_TECNICO)
    primero = rondas[0]["delta"] if rondas else None
    score_final = None if tecnico else scoring.score(case["id"], final)
    score_primero = None if tecnico else scoring.score(case["id"], primero)
    return {
        **record, "status": fallos[0] if fallos else "completed", "technical_failure": tecnico,
        "rounds": len(rondas), "first_delta": primero, "first_score": score_primero,
        "final_delta": final, "score": score_final,
        "final_invalid": None if tecnico else not score_final["valid_state"],
        "repaired": None if tecnico else (not score_primero["success"] and score_final["success"]),
        "elapsed_seconds": sum(r["elapsed_seconds"] for r in rondas),
        "usage": [r["usage"] for r in rondas], "tool_calls": [r["tool_calls"] for r in rondas],
        "mcp_calls": sum(len(r["tool_calls"]) for r in rondas),
        "prompt_sha256": [r["prompt_sha256"] for r in rondas],
        **extra,
    }


def summarize(records):
    por_brazo = {}
    for arm in ARMS:
        filas = [r for r in records if r["arm"] == arm]
        validas = [r for r in filas if not r["technical_failure"]]
        por_brazo[arm] = {
            "runs": len(filas),
            "technical_failures": len(filas) - len(validas),
            "scored_runs": len(validas),
            "final_success": sum(r["score"]["success"] for r in validas),
            "final_invalid_state": sum(r["final_invalid"] for r in validas),
            "first_delta_success": sum(r["first_score"]["success"] for r in validas),
            "repaired": sum(r["repaired"] for r in validas),
            "escalated": sum((r.get("orquestador") or {}).get("resultado") == "escalado" for r in validas),
            "rounds": sum(r["rounds"] for r in validas),
            "mcp_calls": sum(r.get("mcp_calls") or 0 for r in filas),
            # Sensibilidad prerregistrada sobre todas las filas: un fallo técnico
            # cuenta como fracaso en el éxito y, en el peor caso, como estado
            # inválido.
            "final_success_all_rows": sum(r["score"]["success"] for r in validas),
            "final_invalid_state_worst_case": sum(r["final_invalid"] for r in validas) + len(filas) - len(validas),
            "technical_failure_causes": dict(sorted(Counter(
                (r.get("orquestador") or {}).get("motivo") or r["status"]
                for r in filas if r["technical_failure"]).items())),
        }
    return {"schema": PROTOCOL + "-summary", "arms": por_brazo}


def _sha256(path):
    try:
        return hashlib.sha256(Path(path).read_bytes()).hexdigest()
    except OSError:
        return None


def _git(*argumentos):
    resultado = subprocess.run(["git", *argumentos], cwd=REPO, capture_output=True, text=True, check=False)
    return resultado.stdout.strip() if resultado.returncode == 0 else None


def run_manifest(args, plan):
    """Lo que el plan congelado no puede fijar: binarios, versión del agente y commit."""
    codex = None
    if args.agente == "codex":
        codex = subprocess.check_output([str(Path(args.codex_dir) / "bin/codex"), "--version"], text=True,
                                        stderr=subprocess.DEVNULL).strip()
    return {"schema": PROTOCOL + "-run", "fase": args.fase, "agente": args.agente, "plan_sha256": digest(plan),
            # Los resultados de corridas en curso no cuentan como árbol sucio.
            "git_commit": _git("rev-parse", "HEAD"),
            "git_dirty": bool(_git("status", "--porcelain", "--", ".", ":(exclude)evaluation/results")),
            "universo_sha256": _sha256(args.binary), "universo_mcp_sha256": _sha256(args.mcp_binary),
            "codex_cli_version": codex}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", default="gpt-6-astra")
    parser.add_argument("--agente", choices=["codex", "guionizado"], required=True)
    parser.add_argument("--codex-dir")
    parser.add_argument("--mcp-binary", default=str(REPO / "target/debug" / ("universo-mcp.exe" if os.name == "nt" else "universo-mcp")))
    parser.add_argument("--binary", default=str(REPO / "target/debug" / ("universo.exe" if os.name == "nt" else "universo")))
    parser.add_argument("--output", type=Path, help="sólo con --agente guionizado; con codex el destino es fijo")
    parser.add_argument("--fase", choices=["calibracion", "estudio"], default="estudio")
    parser.add_argument("--plan-only", action="store_true", help="escribe el plan y termina")
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()

    plan = build_plan(args.model)
    if args.plan_only:
        destino = args.output or PLAN
        destino.parent.mkdir(parents=True, exist_ok=True)
        destino.write_text(json.dumps(plan, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"planned_runs": plan["runs"], "plan": str(destino)}))
        return 0
    # El plan congelado en el repositorio es el contrato: cualquier cambio de
    # fuentes, tareas o calendario obliga a regenerarlo y revisarlo antes.
    if json.loads(PLAN.read_text(encoding="utf-8")) != plan:
        raise SystemExit("plan_changed_refusing_to_run")
    if args.agente == "codex":
        if not args.codex_dir:
            raise SystemExit("--codex-dir es obligatorio con --agente codex")
        # Un único destino por fase: no se puede correr el estudio en otro
        # directorio y quedarse con la mejor corrida.
        fijo = RESULTS / f"four-arms-v1-{args.fase}"
        if args.output is not None and args.output.resolve() != fijo.resolve():
            raise SystemExit(f"con --agente codex el destino es {fijo}")
        args.output = fijo
        agente = AgenteCodex(args.codex_dir, args.mcp_binary, args.model)
        try:
            agente.preflight()
        except ErrorDeEntorno as error:
            raise SystemExit(f"entorno: {error}") from error
    else:
        if args.output is None:
            raise SystemExit("--output es obligatorio con --agente guionizado")
        import test_four_arms_dataset as referencias

        agente = AgenteGuionizado(lambda case_id, ronda, texto: referencias.REFERENCES[case_id])
    manifest = run_manifest(args, plan)
    if args.agente == "codex" and manifest["git_dirty"]:
        raise SystemExit("working_tree_dirty: commitea antes de una corrida real")
    if args.agente == "codex" and args.fase == "estudio":
        # 9c corre con los mismos binarios y la misma versión de Codex que la
        # calibración 9b, si existe.
        calibracion = RESULTS / "four-arms-v1-calibracion" / "run.json"
        if calibracion.exists():
            previo = json.loads(calibracion.read_text(encoding="utf-8"))
            claves = ["universo_sha256", "universo_mcp_sha256", "codex_cli_version"]
            if {k: previo.get(k) for k in claves} != {k: manifest[k] for k in claves}:
                raise SystemExit("binarios_distintos_de_la_calibracion")
    tasks, _ = scoring.load()
    cases = {c["id"]: c for c in tasks["cases"]}
    # Calibración: sólo el baseline, dos repeticiones; se publica aparte y no
    # entra en el análisis del estudio.
    rows = ([(c, r, a) for c, r, a in schedule(tasks["cases"], SEMILLA, 2) if a == "A_baseline"]
            if args.fase == "calibracion" else [(s["case"], s["repetition"], s["arm"]) for s in plan["schedule"]])
    if len(rows) > MAX_CORRIDAS:
        raise SystemExit("presupuesto_de_corridas_excedido")
    args.output.mkdir(parents=True, exist_ok=True)
    manifest_path = args.output / "run.json"
    if manifest_path.exists():
        previo = json.loads(manifest_path.read_text(encoding="utf-8"))
        claves = ["fase", "agente", "plan_sha256", "universo_sha256", "universo_mcp_sha256", "codex_cli_version"]
        if {k: previo.get(k) for k in claves} != {k: manifest[k] for k in claves}:
            raise SystemExit("run_manifest_changed_refusing_to_resume")
        # Cada reanudación deja rastro del commit desde el que se hizo.
        previo.setdefault("reanudaciones", []).append({"git_commit": manifest["git_commit"],
                                                       "git_dirty": manifest["git_dirty"]})
        manifest_path.write_text(json.dumps(previo, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    else:
        manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    results = args.output / "results.jsonl"
    previous = [json.loads(line) for line in results.read_text(encoding="utf-8").splitlines()] if results.exists() else []
    if previous and not args.resume:
        raise SystemExit("results_exist_use_resume")
    done = {(r["fase"], r["case_id"], r["repetition"], r["arm"]) for r in previous}
    contexto = {"fase": args.fase, "plan_sha256": manifest["plan_sha256"]}
    for case_id, repetition, arm in rows:
        if (args.fase, case_id, repetition, arm) in done:
            continue
        try:
            record = execute(cases[case_id], repetition, arm, agente, args.binary, contexto)
        except ErrorDeEntorno as error:
            # No se escribe la fila: al arreglar el entorno, `--resume` la corre.
            raise SystemExit(f"entorno: {error}") from error
        with results.open("a", encoding="utf-8") as output:
            output.write(json.dumps(record, ensure_ascii=False) + "\n")
        print(json.dumps({k: record[k] for k in ["case_id", "repetition", "arm", "status", "rounds"]}), flush=True)
    records = [json.loads(line) for line in results.read_text(encoding="utf-8").splitlines()]
    (args.output / "summary.json").write_text(json.dumps(summarize(records), ensure_ascii=False, indent=2) + "\n",
                                              encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
