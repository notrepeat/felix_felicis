#!/usr/bin/env python3
"""Medición de la skill ``felix-criterio`` (fase 10, U13) sobre un corpus ficticio.

Para cada caso de ``criterio_deteccion/corpus.json`` el agente recibe la skill
como instrucción de sistema y la conversación, y clasifica el turno objetivo
como ``explicita``, ``implicita`` o ``ninguna``. Se informan:

- **precisión**: de lo que el agente marcó como criterio, cuánto lo era (un
  falso positivo ensucia la base; la meta del plan es ≥ 0,9);
- **cobertura**: del criterio del corpus, cuánto marcó;
- **categoría**: entre los aciertos, cuántos con la categoría correcta.

Las corridas reales usan Claude Code (``claude -p``) aislado con bubblewrap,
como el estudio de cuatro brazos, con la suscripción de Claude del usuario, y
necesitan su decisión explícita. ``--agente oraculo`` recorre el mismo camino
con las etiquetas del corpus, sin LLM, para probar el runner.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
CORPUS = HERE / "criterio_deteccion" / "corpus.json"
SKILL = REPO / "skills" / "felix-criterio" / "SKILL.md"
RESULTS = HERE / "results"
CATEGORIAS = ("explicita", "implicita", "ninguna")
MODELO = "claude-sonnet-5-5"
TIMEOUT_S = 120
META_PRECISION = 0.9
SCHEMA = {
    "type": "object",
    "additionalProperties": False,
    "required": ["categoria", "afirmacion"],
    "properties": {
        "categoria": {"type": "string", "enum": list(CATEGORIAS)},
        "afirmacion": {"type": "string"},
    },
}
INSTRUCCION = (
    "Estás evaluando una conversación ficticia, no trabajando en un proyecto: no captures nada ni ejecutes "
    "comandos. Clasifica sólo el turno indicado según la guía de arriba y entrega tu respuesta con la herramienta "
    "de salida estructurada: `categoria` (explicita, implicita o ninguna) y `afirmacion` (la paráfrasis que "
    "guardarías, o cadena vacía si es ninguna). Si dudarías y preguntarías al dueño, la categoría es ninguna."
)


class ErrorDeEntorno(RuntimeError):
    """El entorno no permite correr (sin bwrap, sin login, sin cuota)."""


def sha256(ruta):
    return hashlib.sha256(Path(ruta).read_bytes()).hexdigest()


def cargar_corpus(ruta=CORPUS):
    """Casos del corpus, validados: ids únicos, objetivo en rango, categoría conocida."""
    datos = json.loads(Path(ruta).read_text(encoding="utf-8"))
    if datos.get("schema") != "felix-criterio-deteccion-v1":
        raise ValueError("corpus_schema")
    vistos = set()
    for caso in datos["casos"]:
        if caso["id"] in vistos:
            raise ValueError(f"corpus_id_duplicado:{caso['id']}")
        vistos.add(caso["id"])
        if caso["categoria"] not in CATEGORIAS:
            raise ValueError(f"corpus_categoria:{caso['id']}")
        turnos = caso["turnos"]
        if not 0 <= caso["objetivo"] < len(turnos):
            raise ValueError(f"corpus_objetivo:{caso['id']}")
        if any(t["rol"] not in ("dueño", "agente") or not t["texto"].strip() for t in turnos):
            raise ValueError(f"corpus_turno:{caso['id']}")
    return datos["casos"]


def cargar_skill(ruta=SKILL):
    """Cuerpo de la skill, sin el frontmatter."""
    texto = Path(ruta).read_text(encoding="utf-8")
    if not texto.startswith("---\n"):
        raise ValueError("skill_sin_frontmatter")
    _, _, cuerpo = texto[4:].partition("\n---\n")
    if not cuerpo.strip():
        raise ValueError("skill_vacia")
    return cuerpo.strip()


def prompt(caso):
    lineas = ["Conversación:"]
    for i, turno in enumerate(caso["turnos"]):
        lineas.append(f"[{i}] {turno['rol']}: {turno['texto']}")
    lineas.append("")
    lineas.append(f"Turno a clasificar: [{caso['objetivo']}].")
    return "\n".join(lineas)


def sistema(skill):
    return f"{skill}\n\n---\n\n{INSTRUCCION}"


def wilson(aciertos, total, z=1.96):
    """Intervalo de Wilson al 95 % para una proporción, o None sin datos."""
    if not total:
        return None
    p = aciertos / total
    centro = (p + z * z / (2 * total)) / (1 + z * z / total)
    margen = z * ((p * (1 - p) / total + z * z / (4 * total * total)) ** 0.5) / (1 + z * z / total)
    return [round(max(0.0, centro - margen), 4), round(min(1.0, centro + margen), 4)]


def puntuar(casos, respuestas):
    """Métricas a partir de las etiquetas y las categorías predichas (por id).

    Un caso sin respuesta válida (timeout, fallo de ejecución, salida fuera del
    esquema) no cuenta como «ninguna»: se excluye de las métricas y la corrida
    queda marcada como no válida, para que un fallo técnico no infle la
    precisión."""
    tp = fp = fn = tn = categoria_ok = 0
    errores = []
    sin_respuesta = [c["id"] for c in casos if respuestas.get(c["id"]) is None]
    for caso in casos:
        real = caso["categoria"]
        pred = respuestas.get(caso["id"])
        if pred is None:
            continue
        es, dice = real != "ninguna", pred != "ninguna"
        if es and dice:
            tp += 1
            if pred == real:
                categoria_ok += 1
            else:
                errores.append({"id": caso["id"], "tipo": "categoria", "real": real, "pred": pred})
        elif dice:
            fp += 1
            errores.append({"id": caso["id"], "tipo": "falso_positivo", "real": real, "pred": pred})
        elif es:
            fn += 1
            errores.append({"id": caso["id"], "tipo": "falso_negativo", "real": real, "pred": pred})
        else:
            tn += 1
    return {
        "casos": len(casos),
        "valido": not sin_respuesta,
        "verdaderos_positivos": tp,
        "falsos_positivos": fp,
        "falsos_negativos": fn,
        "verdaderos_negativos": tn,
        "precision": tp / (tp + fp) if tp + fp else None,
        "precision_ic95": wilson(tp, tp + fp),
        "cobertura": tp / (tp + fn) if tp + fn else None,
        "cobertura_ic95": wilson(tp, tp + fn),
        "categoria_correcta": categoria_ok / tp if tp else None,
        "sin_respuesta": sin_respuesta,
        "errores": errores,
    }


sys.path.insert(0, str(HERE))
from run_paired import bounded_capture, parse_events  # noqa: E402
from run_four_arms import fallo_del_proveedor, interpretar_claude  # noqa: E402


class AgenteOraculo:
    """Responde la etiqueta del corpus: prueba el camino completo sin LLM."""

    nombre = "oraculo"

    def __init__(self, casos):
        self.etiquetas = {c["id"]: c["categoria"] for c in casos}
        self.llamadas = []

    def preflight(self):
        pass

    def version(self):
        return "oraculo"

    def clasificar(self, caso, sistema_texto, texto):
        self.llamadas.append({"id": caso["id"], "sistema": sistema_texto, "prompt": texto})
        return {"categoria": self.etiquetas[caso["id"]], "afirmacion": ""}, "ok"


GESTORES_DE_SHIMS = ("mise",)


def binario_real(ruta):
    """Ruta real del binario de Claude. Un shim de mise (enlace a ``mise``)
    no sirve dentro de bwrap, que no ve su configuración: se le pregunta a
    mise qué binario ejecutaría. Otros envoltorios (scripts de asdf, un
    ``cli.js`` de npm) no se resuelven; ``version()`` los rechaza antes del
    primer caso porque no arrancan en el aislamiento."""
    ruta = Path(ruta)
    resuelto = ruta.resolve()
    if resuelto.name not in GESTORES_DE_SHIMS:
        return resuelto
    try:
        salida = subprocess.run([str(resuelto), "which", ruta.name], capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ErrorDeEntorno(f"claude_shim_unresolved: {type(error).__name__}") from error
    if salida.returncode != 0 or not salida.stdout.strip():
        raise ErrorDeEntorno(f"claude_shim_unresolved: {resuelto.name}")
    return Path(salida.stdout.strip()).resolve()


class AgenteClaude:
    """Claude Code en modo no interactivo, aislado con bubblewrap.

    Como en el estudio de cuatro brazos: HOME temporal, sin ajustes, hooks,
    plugins, memoria ni CLAUDE.md del usuario (``--setting-sources ""``), sin
    herramientas (``--tools ""``; sólo queda la salida estructurada) y sin MCP.
    """

    nombre = "claude"

    def __init__(self, claude_binary, model, timeout=TIMEOUT_S):
        self.claude_binary = binario_real(claude_binary)
        self.model = model
        self.timeout = timeout

    @staticmethod
    def credenciales():
        return Path.home() / ".claude/.credentials.json"

    def aislamiento(self, home):
        """Prefijo bwrap: sistema de sólo lectura, un HOME vacío con las
        credenciales y el binario de Claude en ``/opt/claude/claude``."""
        cmd = ["bwrap", "--unshare-all", "--share-net", "--die-with-parent", "--new-session", "--clearenv"]
        for path in ["/usr", "/bin", "/lib", "/lib64", "/etc/ssl", "/etc/ca-certificates", "/etc/resolv.conf",
                     "/etc/hosts", "/etc/nsswitch.conf"]:
            if Path(path).exists():
                cmd += ["--ro-bind", path, path]
        cmd += ["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp",
                "--bind", str(home), "/agente-home",
                "--bind", str(self.credenciales()), "/agente-home/.claude/.credentials.json",
                "--ro-bind", str(self.claude_binary), "/opt/claude/claude", "--chdir", "/tmp",
                "--setenv", "HOME", "/agente-home", "--setenv", "PATH", "/usr/bin:/bin", "--setenv", "LANG", "C.UTF-8"]
        return cmd

    def comando_version(self, home):
        return self.aislamiento(home) + ["/opt/claude/claude", "--version"]

    def command(self, home, sistema_texto):
        cmd = self.aislamiento(home) + [
                "/opt/claude/claude", "-p", "--output-format", "stream-json", "--verbose",
                "--model", self.model, "--tools", "", "--setting-sources", "", "--strict-mcp-config",
                "--no-session-persistence", "--max-turns", "3",
                "--system-prompt", sistema_texto, "--json-schema", json.dumps(SCHEMA)]
        return cmd

    def preflight(self):
        if shutil.which("bwrap") is None:
            raise ErrorDeEntorno("bwrap_required")
        if not self.credenciales().is_file():
            raise ErrorDeEntorno("claude_login_required")
        if not self.claude_binary.is_file():
            raise ErrorDeEntorno("missing_claude")

    def version(self):
        """Pide la versión dentro del mismo aislamiento que la corrida: si el
        binario no arranca ahí, la corrida se corta antes del primer caso."""
        with tempfile.TemporaryDirectory(prefix="felix-criterio-") as temporal:
            home = Path(temporal) / "home"
            (home / ".claude").mkdir(parents=True)
            try:
                proceso = subprocess.run(self.comando_version(home), capture_output=True, text=True,
                                         timeout=60, env={"PATH": "/usr/bin:/bin"})
            except (OSError, subprocess.TimeoutExpired) as error:
                raise ErrorDeEntorno(f"claude_sandbox_failed: {type(error).__name__}") from error
        if proceso.returncode != 0:
            detalle = (proceso.stderr or proceso.stdout).strip()[-300:]
            raise ErrorDeEntorno(f"claude_sandbox_failed: {detalle}")
        return proceso.stdout.strip()

    def clasificar(self, caso, sistema_texto, texto):
        """(salida estructurada o None, estado): ``ok``, ``timeout``,
        ``output_limit``, ``execution_failed`` o ``sin_salida``. Un fallo
        transitorio del proveedor se reintenta una vez; un límite de uso corta
        la corrida."""
        try:
            return self._clasificar_una_vez(sistema_texto, texto)
        except ErrorDeEntorno as error:
            if str(error) != "provider_turn_failed":
                raise
        return self._clasificar_una_vez(sistema_texto, texto)

    def _clasificar_una_vez(self, sistema_texto, texto):
        with tempfile.TemporaryDirectory(prefix="felix-criterio-") as temporal:
            home = Path(temporal) / "home"
            (home / ".claude").mkdir(parents=True)
            try:
                proceso = subprocess.Popen(self.command(home, sistema_texto), stdin=subprocess.PIPE,
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
                                           env={"PATH": "/usr/bin:/bin"})
            except OSError as error:
                raise ErrorDeEntorno(f"spawn_failed_{type(error).__name__}") from error
            stdout, _stderr, timed_out, overflow = bounded_capture(proceso, texto.encode("utf-8"), self.timeout)
            eventos = parse_events(stdout.decode(errors="replace"))
            proveedor = fallo_del_proveedor(eventos)
            if proveedor is not None:
                raise ErrorDeEntorno(proveedor)
            if timed_out:
                return None, "timeout"
            if overflow:
                return None, "output_limit"
            if proceso.returncode != 0:
                return None, "execution_failed"
            salida, _usos = interpretar_claude(eventos)
            return salida, ("ok" if salida is not None else "sin_salida")


def correr(agente, casos, skill):
    """Clasifica cada caso. Devuelve las filas, las métricas y el error de
    entorno que cortó la corrida (o None): lo ya clasificado no se pierde."""
    sistema_texto = sistema(skill)
    filas, respuestas, corte = [], {}, None
    for caso in casos:
        inicio = time.monotonic()
        try:
            salida, estado = agente.clasificar(caso, sistema_texto, prompt(caso))
        except ErrorDeEntorno as error:
            corte = str(error)
            break
        categoria = salida.get("categoria") if isinstance(salida, dict) else None
        if categoria not in CATEGORIAS:
            categoria = None
            if estado == "ok":
                estado = "fuera_de_esquema"
        respuestas[caso["id"]] = categoria
        filas.append({"id": caso["id"], "real": caso["categoria"], "pred": categoria, "estado": estado,
                      "afirmacion": salida.get("afirmacion") if isinstance(salida, dict) else None,
                      "segundos": round(time.monotonic() - inicio, 2)})
    return filas, puntuar(casos, respuestas), corte


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--agente", choices=["claude", "oraculo"], required=True)
    parser.add_argument("--model", default=MODELO)
    parser.add_argument("--claude-binary", default=shutil.which("claude") or "claude")
    parser.add_argument("--corpus", default=str(CORPUS))
    parser.add_argument("--skill", default=str(SKILL))
    parser.add_argument("--salida", help="JSON de resultados (por defecto, en evaluation/results)")
    args = parser.parse_args(argv)

    casos = cargar_corpus(args.corpus)
    skill = cargar_skill(args.skill)
    try:
        agente = AgenteOraculo(casos) if args.agente == "oraculo" else AgenteClaude(args.claude_binary, args.model)
        agente.preflight()
        version = agente.version()
    except (ErrorDeEntorno, OSError, subprocess.CalledProcessError) as error:
        print(f"error de entorno: {error}", file=sys.stderr)
        return 2
    filas, metricas, corte = correr(agente, casos, skill)
    resultado = {
        "schema": "felix-criterio-deteccion-resultados-v1",
        "agente": agente.nombre,
        "modelo": args.model if args.agente == "claude" else None,
        "version_agente": version,
        "fecha": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "corpus_sha256": sha256(args.corpus),
        "skill_sha256": sha256(args.skill),
        "meta_precision": META_PRECISION,
        "completo": corte is None,
        "corte": corte,
        "metricas": metricas,
        "casos": filas,
    }
    salida = Path(args.salida) if args.salida else RESULTS / f"criterio-deteccion-{agente.nombre}.json"
    salida.parent.mkdir(parents=True, exist_ok=True)
    salida.write_text(json.dumps(resultado, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    def fmt(valor):
        return "n/a" if valor is None else f"{valor:.3f}"

    print(f"precisión {fmt(metricas['precision'])} (IC95 {metricas['precision_ic95']}) · "
          f"cobertura {fmt(metricas['cobertura'])} · categoría {fmt(metricas['categoria_correcta'])} · "
          f"{metricas['casos']} casos -> {salida}")
    if corte is not None:
        print(f"corrida cortada: {corte}; {len(filas)} casos clasificados", file=sys.stderr)
        return 2
    if not metricas["valido"]:
        print(f"corrida no válida: sin respuesta en {metricas['sin_respuesta']}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
