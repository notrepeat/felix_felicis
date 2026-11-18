#!/usr/bin/env python3
"""Prototipo de orquestador obligatorio (paso 8, propuesta 08).

El agente nunca escribe el bundle: recibe una copia desechable y devuelve un
delta. El orquestador lo verifica siempre con `universo propose --explain`,
devuelve el rechazo redactado con un presupuesto de reparación y sólo el
ejecutor aplica, con CAS sobre el snapshot verificado. `universo guard` corre
al empezar, tras cada turno del agente, justo antes del apply y después.

Un error técnico, un timeout, una excepción del agente o un presupuesto
agotado terminan en `escalado`: nunca se interpretan como admisión. Si el apply
ya hizo commit, el resultado es `publicado` aunque la comprobación posterior
falle (va en `alertas`); si no se sabe si hizo commit, es `incierto`.

El aislamiento es por construcción (el agente no recibe la ruta real), no por
el sistema operativo; una fuga de ruta queda cubierta por la detección de
`guard`, como muestra el agente `intruso` de la demo.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import hmac
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unicodedata
from typing import Any, Callable, Optional, Protocol
import uuid

TIEMPO_LIMITE_S = 60
PRESUPUESTO_POR_DEFECTO = 3
# Mismo límite que `universo` impone al motivo del historial.
MAX_OBJETIVO = 500


@dataclass(frozen=True)
class Repositorio:
    """Rutas de un repositorio de conocimiento: bundle, manifiesto e invariantes."""

    raiz: Path
    manifiesto: Path
    invariantes: Path


class Agente(Protocol):
    def proponer(self, vista: Path, retroalimentacion: Optional[dict]) -> Any:
        """Devuelve un delta (objeto JSON) a partir de la vista y del último rechazo."""


class _Escalado(Exception):
    def __init__(self, motivo: str):
        super().__init__(motivo)
        self.motivo = motivo


class _Incierto(Exception):
    """El apply se lanzó y no se sabe si hizo commit."""


def validar_objetivo(objetivo: Any) -> str:
    """El objetivo va al historial (`--reason`): texto de una línea, imprimible y acotado."""
    if (
        not isinstance(objetivo, str)
        or not objetivo.strip()
        or len(objetivo) > MAX_OBJETIVO
        or any(not c.isprintable() or unicodedata.category(c) == "Cf" for c in objetivo)
    ):
        raise ValueError(f"el objetivo debe ser texto imprimible de una línea, 1 a {MAX_OBJETIVO} caracteres")
    return objetivo


def _universo(binario: Path, argumentos: list[str]) -> tuple[int, dict]:
    """Ejecuta la CLI con salida JSON; cualquier fallo de proceso es técnico."""
    try:
        proceso = subprocess.run(
            [str(binario), *argumentos, "--format=json"],
            capture_output=True,
            timeout=TIEMPO_LIMITE_S,
            check=False,
        )
        salida = json.loads(proceso.stdout)
    except (OSError, subprocess.SubprocessError, ValueError) as error:
        raise _Escalado("error_tecnico") from error
    if not isinstance(salida, dict):
        raise _Escalado("error_tecnico")
    return proceso.returncode, salida


def _codigos(salida: dict) -> list[str]:
    return sorted({e["code"] for e in salida.get("errors") or [] if isinstance(e, dict) and isinstance(e.get("code"), str)})


# Errores de forma del propio delta, que el agente puede corregir. Es una lista
# blanca: cualquier otro código (lock, stage, presupuesto del motor, replay,
# snapshot...) es técnico y escala, nunca se devuelve como reparable.
CODIGOS_DE_DELTA = frozenset({
    "add_conflict", "remove_not_found", "unsupported_operation", "delta_invalid",
    "label_missing", "label_orphan", "label_contains_line_break", "label_replacement_required",
    "subject_not_found", "child_reference_not_found", "parent_reference_not_found",
    "parent_cycle", "self_parent", "unknown_node_sort", "unknown_predicate_domain",
})


def _es_error_de_delta(codigo: str) -> bool:
    return codigo in CODIGOS_DE_DELTA


def formas_de_nivel_superior(texto: str) -> list[str]:
    """Las S-expresiones de nivel superior de un programa `.sx`, con comentarios `;` y cadenas."""
    formas, profundidad, inicio, i, en_cadena = [], 0, None, 0, False
    while i < len(texto):
        c = texto[i]
        if en_cadena:
            if c == "\\":
                i += 1
            elif c == '"':
                en_cadena = False
        elif c == '"':
            en_cadena = True
        elif c == ";":
            while i < len(texto) and texto[i] != "\n":
                i += 1
            continue
        elif c == "(":
            if profundidad == 0:
                inicio = i
            profundidad += 1
        elif c == ")":
            profundidad -= 1
            if profundidad == 0 and inicio is not None:
                formas.append(texto[inicio:i + 1])
                inicio = None
            if profundidad < 0:
                raise ValueError("paréntesis desbalanceados")
        i += 1
    if profundidad != 0 or en_cadena:
        raise ValueError("programa .sx incompleto")
    return formas


def _leer_cadena(texto: str, i: int) -> tuple[str, int]:
    """Lee una cadena `"..."` que empieza en `i` (escapes `\\` y `\"`, como el lexer de Felix)."""
    salida, i = [], i + 1
    while i < len(texto):
        c = texto[i]
        if c == "\\" and i + 1 < len(texto) and texto[i + 1] in '"\\':
            salida.append(texto[i + 1])
            i += 2
        elif c == '"':
            return "".join(salida), i + 1
        else:
            salida.append(c)
            i += 1
    raise ValueError("cadena sin cerrar")


def metadatos_de_regla(forma: str) -> tuple[Optional[str], Optional[str]]:
    """`:id` y `:mensaje` de una forma `(regla ...)` en cualquier orden, o `(None, None)`."""
    if not re.match(r"\(regla\s", forma):
        return None, None
    valores: dict[str, str] = {}
    i = len("(regla")
    while True:
        while i < len(forma) and forma[i].isspace():
            i += 1
        clave = next((k for k in (":id", ":mensaje") if forma.startswith(k, i)), None)
        if clave is None:
            break
        i += len(clave)
        while i < len(forma) and forma[i].isspace():
            i += 1
        if i >= len(forma) or forma[i] != '"':
            break
        valores[clave], i = _leer_cadena(forma, i)
    return valores.get(":id"), valores.get(":mensaje")


def _argumentos_repo(repo: Repositorio) -> list[str]:
    return [str(repo.raiz), f"--manifiesto={repo.manifiesto}", f"--invariantes={repo.invariantes}"]


def guardia(binario: Path, repo: Repositorio) -> dict:
    """`universo guard`: `ok` sólo si no hay transacción pendiente, se cumplen las
    invariantes y el estado coincide con el sello del último admitido."""
    codigo, salida = _universo(binario, ["guard", *_argumentos_repo(repo)])
    return {**salida, "ok": codigo == 0 and salida.get("status") == "pass"}


def crear_repositorio_demo(binario: Path, directorio: Path) -> Repositorio:
    """Crea con `universo init` un repositorio mínimo, válido y sellado."""
    codigo, salida = _universo(binario, ["init", str(directorio)])
    if codigo != 0 or salida.get("status") != "ok":
        raise RuntimeError("universo init falló")
    return Repositorio(directorio / "bundle", directorio / "manifest.toml", directorio / "invariants.sx")


class Orquestador:
    def __init__(
        self,
        binario: Path,
        repo: Repositorio,
        *,
        registro: Optional[Path] = None,
        presupuesto: int = PRESUPUESTO_POR_DEFECTO,
        confirmar: Optional[Callable[[dict], bool]] = None,
        reparar_errores_de_delta: bool = False,
        testigo: bool = False,
    ):
        if presupuesto < 1:
            raise ValueError("el presupuesto de intentos debe ser al menos 1")
        self.binario = Path(binario)
        self.repo = repo
        self.registro = registro
        self.presupuesto = presupuesto
        self.confirmar = confirmar
        # Con la opción activa, un error de forma del delta (`add_conflict`,
        # `remove_not_found`, `label_missing`...) vuelve al agente como un
        # rechazo más, consumiendo un intento. Nunca se convierte en admisión.
        self.reparar_errores_de_delta = reparar_errores_de_delta
        # Con `testigo`, el rechazo incluye la primera asignación que incumple
        # la regla (`propose --witness`): rutas del bundle, sólo uso local.
        self.testigo = testigo
        # Los hashes del registro llevan una clave aleatoria por orquestador:
        # sirven para correlacionar intentos, no para adivinar el contenido
        # probando candidatos.
        self._clave = os.urandom(32)

    def _huella(self, datos: Any) -> str:
        texto = json.dumps(datos, sort_keys=True, ensure_ascii=False, default=repr)
        return hmac.new(self._clave, texto.encode("utf-8"), hashlib.sha256).hexdigest()

    def _anotar(self, episodio: str, evento: str, **campos: Any) -> None:
        # El registro no guarda contenido del bundle: sólo hashes, veredictos,
        # ids de regla y conteos.
        if self.registro is None:
            return
        with open(self.registro, "a", encoding="utf-8") as archivo:
            archivo.write(json.dumps({"episodio": episodio, "evento": evento, **campos}, ensure_ascii=False) + "\n")

    def _exigir_guardia(self, motivo: str) -> None:
        estado = guardia(self.binario, self.repo)
        if estado["ok"]:
            return
        # Una transacción en curso (otro orquestador, un apply interrumpido)
        # no es una escritura externa: se informa aparte.
        raise _Escalado("transaccion_pendiente" if estado.get("transaction") == "pending" else motivo)

    def _vista(self, destino: Path) -> Path:
        """Copia desechable que recibe el agente; lo que escriba ahí se descarta."""
        vista = destino / "vista"
        shutil.copytree(self.repo.raiz, vista / "bundle")
        shutil.copy2(self.repo.manifiesto, vista / "manifest.toml")
        shutil.copy2(self.repo.invariantes, vista / "invariants.sx")
        return vista

    def _simular(self, archivo_delta: Path) -> dict:
        codigo, salida = _universo(
            self.binario,
            [
                "propose",
                *_argumentos_repo(self.repo),
                f"--delta={archivo_delta}",
                "--explain",
                "decide",
                "--rules",
                "--emit-snapshot-token",
                *(["--witness"] if self.testigo else []),
            ],
        )
        estado = salida.get("status")
        token = salida.get("snapshot_token")
        if estado == "admit" and codigo == 0 and isinstance(token, str):
            return salida
        if estado == "reject" and codigo == 1 and isinstance(salida.get("explanations"), dict):
            return salida
        if (estado == "reject" and codigo == 1 and isinstance(token, str)
                and salida.get("explanation_error") == "explanation_budget_exceeded"):
            # En bundles grandes la explicación redactada puede agotar su
            # presupuesto. El veredicto ya es `reject`; para decirle al agente
            # qué reglas fallan se simula el delta contra cada regla por
            # separado, sin explicación.
            salida["explanations"] = {"objects": self._reglas_fallidas(archivo_delta), "fallback": "per_rule"}
            return salida
        if self.reparar_errores_de_delta and estado == "error" and codigo == 1:
            codigos = _codigos(salida)
            if codigos and all(_es_error_de_delta(c) for c in codigos):
                return salida
        raise _Escalado("error_tecnico")

    def _reglas_fallidas(self, archivo_delta: Path) -> list[dict]:
        texto = self.repo.invariantes.read_text(encoding="utf-8")
        fallidas = []
        with tempfile.TemporaryDirectory(prefix="felix-orquestador-regla-") as nombre:
            for indice, forma in enumerate(formas_de_nivel_superior(texto)):
                archivo = Path(nombre) / f"regla-{indice}.sx"
                archivo.write_text(forma + "\n", encoding="utf-8")
                codigo, salida = _universo(
                    self.binario,
                    ["propose", str(self.repo.raiz), f"--manifiesto={self.repo.manifiesto}",
                     f"--invariantes={archivo}", f"--delta={archivo_delta}"],
                )
                if salida.get("status") == "reject" and codigo == 1:
                    rule_id, rule_message = metadatos_de_regla(forma)
                    fallidas.append({"kind": "rule", "source_index": indice, "rule_id": rule_id,
                                     "rule_message": rule_message})
                    # Igual que la explicación `decide` del motor: sólo la
                    # primera regla fallida en orden de fuente, para no dar
                    # más diagnóstico que el camino normal.
                    break
                if salida.get("status") != "admit":
                    raise _Escalado("error_tecnico")
        if not fallidas:
            # El conjunto rechaza pero ninguna regla sola lo hace: no debería
            # pasar con reglas independientes; no se inventa un diagnóstico.
            raise _Escalado("error_tecnico")
        return fallidas

    def _aplicar(self, archivo_delta: Path, token: str, objetivo: str) -> dict:
        try:
            codigo, salida = _universo(
                self.binario,
                [
                    "propose",
                    *_argumentos_repo(self.repo),
                    f"--delta={archivo_delta}",
                    "--apply",
                    f"--expect-snapshot={token}",
                    "--author=orquestador",
                    f"--reason={objetivo}",
                ],
            )
        except _Escalado as error:
            # Timeout o salida ilegible: el proceso pudo hacer commit o no.
            raise _Incierto() from error
        if codigo == 0 and salida.get("status") == "admit" and salida.get("mode") == "apply":
            return salida
        codigos = {e.get("code") for e in salida.get("errors") or [] if isinstance(e, dict)}
        # Sólo dos salidas garantizan que el bundle no se tocó: el CAS falló
        # (`stale`, antes de preparar) o la reevaluación bajo lock rechazó
        # (antes de instalar). Cualquier otra —errores `*_unverified` con la
        # transacción iniciada, `admit` con código distinto de 0 porque falló
        # la salida, o un error inesperado— no permite afirmar que no hubo
        # commit.
        if codigo == 1 and salida.get("status") == "error" and codigos == {"stale"}:
            raise _Escalado("snapshot_cambiado")
        if codigo == 1 and salida.get("status") == "reject":
            raise _Escalado("rechazo_al_aplicar")
        raise _Incierto()

    def ejecutar(self, agente: Agente, objetivo: str) -> dict:
        validar_objetivo(objetivo)
        episodio = uuid.uuid4().hex
        self._intentos = 0
        self._publicado = False
        self._alertas: list[str] = []
        try:
            self._anotar(episodio, "inicio", objetivo_hmac=self._huella(objetivo), presupuesto=self.presupuesto)
            resultado, motivo = self._episodio(episodio, agente, objetivo)
        except _Incierto:
            resultado, motivo = "incierto", "apply_sin_confirmar"
        except _Escalado as escalado:
            resultado, motivo = "escalado", escalado.motivo
        except Exception:  # noqa: BLE001 - un fallo interno nunca es admisión
            resultado, motivo = "escalado", "error_interno"
        if self._publicado and resultado != "publicado":
            # El apply ya hizo commit: lo que falló después es una alerta, no
            # un escalado, para que la etiqueta diga lo que pasó en el bundle.
            self._alertas.append(motivo)
            resultado, motivo = "publicado", None
        try:
            self._anotar(
                episodio, "fin", resultado=resultado, motivo=motivo, intentos=self._intentos, alertas=self._alertas
            )
        except OSError:
            self._alertas.append("registro_incompleto")
        return {"resultado": resultado, "motivo": motivo, "intentos": self._intentos, "alertas": self._alertas}

    def _episodio(self, episodio: str, agente: Agente, objetivo: str) -> tuple[str, Optional[str]]:
        self._exigir_guardia("estado_inicial_no_sellado")
        retroalimentacion: Optional[dict] = None
        # La vista del agente y el buzón del orquestador viven en árboles
        # temporales distintos: desde la vista no se deduce la ruta del buzón.
        with tempfile.TemporaryDirectory(
            prefix="felix-orquestador-", ignore_cleanup_errors=True
        ) as nombre_buzon, tempfile.TemporaryDirectory(prefix="felix-vista-", ignore_cleanup_errors=True) as nombre_vistas:
            buzon = Path(nombre_buzon)
            for intento in range(1, self.presupuesto + 1):
                self._intentos = intento
                vista = self._vista(Path(nombre_vistas) / f"intento-{intento}")
                try:
                    propuesto = agente.proponer(vista, retroalimentacion)
                except Exception as error:  # noqa: BLE001 - cualquier fallo del agente escala
                    raise _Escalado("error_agente") from error
                # Una escritura fuera del ejecutor durante el turno del agente
                # invalida el episodio antes de verificar nada.
                self._exigir_guardia("escritura_externa")
                # Copia canónica: lo que se verifica, se confirma y se aplica es
                # este texto, no el objeto que el agente aún podría mutar.
                try:
                    if not isinstance(propuesto, dict):
                        raise TypeError("el delta no es un objeto JSON")
                    texto_delta = json.dumps(propuesto, ensure_ascii=False, sort_keys=True)
                except (TypeError, ValueError) as error:
                    self._anotar(episodio, "veredicto", intento=intento, delta_hmac=self._huella(propuesto), veredicto="error")
                    raise _Escalado("error_tecnico") from error
                huella = self._huella(json.loads(texto_delta))
                archivo_delta = buzon / f"delta-{intento}.json"
                archivo_delta.write_text(texto_delta, encoding="utf-8")
                try:
                    salida = self._simular(archivo_delta)
                except _Escalado:
                    self._anotar(episodio, "veredicto", intento=intento, delta_hmac=huella, veredicto="error")
                    raise
                objetos = salida["explanations"]["objects"] if salida["status"] == "reject" else []
                codigos = _codigos(salida) if salida["status"] == "error" else []
                self._anotar(
                    episodio,
                    "veredicto",
                    intento=intento,
                    delta_hmac=huella,
                    veredicto=salida["status"],
                    reglas=[o.get("rule_id") for o in objetos],
                    codigos=codigos,
                    invariantes_fallidas=(salida.get("summary") or {}).get("failed_invariants"),
                )
                if salida["status"] == "admit":
                    return self._publicar(
                        episodio, buzon, intento, texto_delta, salida["snapshot_token"], objetivo
                    )
                retroalimentacion = {
                    "veredicto": salida["status"],
                    "codigos": codigos,
                    "testigo": salida.get("witness") if self.testigo else None,
                    "intento": intento,
                    "intentos_restantes": self.presupuesto - intento,
                    "objetos": objetos,
                    "resumen": salida.get("summary"),
                }
        raise _Escalado("presupuesto_agotado")

    def _publicar(
        self, episodio: str, buzon: Path, intento: int, texto_delta: str, token: str, objetivo: str
    ) -> tuple[str, Optional[str]]:
        if self.confirmar is not None:
            try:
                aprobado = (
                    self.confirmar({"objetivo": objetivo, "intentos": intento, "delta": json.loads(texto_delta)})
                    is True
                )
            except Exception:  # noqa: BLE001 - una confirmación fallida no autoriza
                aprobado = False
            if not aprobado:
                return "no_confirmado", None
        self._exigir_guardia("escritura_externa")
        # Se aplica desde un archivo nuevo escrito con el texto verificado y
        # confirmado. Lo protege que el agente no recibe la ruta del buzón, no
        # una comprobación en disco: el aislamiento es por construcción.
        archivo = buzon / f"aplicar-{intento}.json"
        archivo.write_text(texto_delta, encoding="utf-8")
        salida = self._aplicar(archivo, token, objetivo)
        self._publicado = True
        sellado = salida.get("sealed") is True
        if not sellado:
            self._alertas.append("sin_sello")
        self._anotar(episodio, "publicacion", sellado=sellado)
        if not guardia(self.binario, self.repo)["ok"]:
            self._alertas.append("guard_final_fallido")
        return "publicado", None


# --- Demo con agentes guionizados -------------------------------------------------

_VALIDO = {
    "schema": "felix-delta-v1",
    "add": ["Estado(notas/bienvenida, vigente)"],
    "remove": ["Estado(notas/bienvenida, borrador)"],
}
_INVALIDO = {
    "schema": "felix-delta-v1",
    "add": ["Type(notas/bienvenida, fuente)", "Estado(notas/bienvenida, vigente)"],
    "remove": ["Type(notas/bienvenida, criterio)", "Estado(notas/bienvenida, borrador)"],
}


class _Correcto:
    def proponer(self, vista, retroalimentacion):
        return _VALIDO


class _Reparador:
    """Empieza mal y corrige leyendo la regla que falló."""

    def proponer(self, vista, retroalimentacion):
        if retroalimentacion is None:
            return _INVALIDO
        reglas = {o.get("rule_id") for o in retroalimentacion["objetos"]}
        return _VALIDO if "vigente-es-criterio" in reglas else _INVALIDO


class _Obstinado:
    def proponer(self, vista, retroalimentacion):
        return _INVALIDO


class _Intruso:
    """Escribe el Markdown real por una ruta filtrada y luego propone algo válido."""

    def __init__(self, nota: Path):
        self.nota = nota

    def proponer(self, vista, retroalimentacion):
        self.nota.write_text(self.nota.read_text(encoding="utf-8") + "\nescrito por fuera\n", encoding="utf-8")
        return _VALIDO


def _efecto_publicado(binario: Path, repo: Repositorio) -> bool:
    """Estado real del bundle: ¿está el átomo que sólo el delta válido introduce?

    No depende de la etiqueta del orquestador ni del historial de Felix (que es
    informativo y puede faltar tras un commit). Una edición de texto del
    intruso no cambia los átomos, así que no cuenta como publicación."""
    proceso = subprocess.run(
        [str(binario), "atomos", str(repo.raiz), f"--manifiesto={repo.manifiesto}"],
        capture_output=True,
        timeout=TIEMPO_LIMITE_S,
        check=True,
    )
    return "Estado(notas/bienvenida, vigente)" in proceso.stdout.decode("utf-8").splitlines()


def _cumple_invariantes(binario: Path, repo: Repositorio) -> bool:
    codigo, salida = _universo(binario, ["check", *_argumentos_repo(repo)])
    return codigo == 0 and salida.get("status") == "admit"


def demo(binario: Path, directorio: Path) -> dict:
    """Corre los cuatro agentes guionizados, cada uno sobre un repositorio nuevo.

    Las métricas miran los átomos reales del bundle y `check`, no la etiqueta
    que devuelve el orquestador."""
    directorio.mkdir(parents=True, exist_ok=True)
    episodios = []
    invalidos_aceptados = 0
    externas_publicadas = 0
    etiquetas_inconsistentes = 0
    for nombre in ("correcto", "reparador", "obstinado", "intruso"):
        repo = crear_repositorio_demo(binario, directorio / nombre)
        if nombre == "intruso":
            agente: Agente = _Intruso(repo.raiz / "notas" / "bienvenida.md")
        else:
            agente = {"correcto": _Correcto, "reparador": _Reparador, "obstinado": _Obstinado}[nombre]()
        if _efecto_publicado(binario, repo):
            raise RuntimeError("el repositorio de la demo no parte del estado inicial")
        resultado = Orquestador(binario, repo, registro=directorio / "registro.jsonl").ejecutar(agente, nombre)
        publico = _efecto_publicado(binario, repo)
        if publico != (resultado["resultado"] == "publicado"):
            etiquetas_inconsistentes += 1
        if publico and not _cumple_invariantes(binario, repo):
            invalidos_aceptados += 1
        if publico and nombre == "intruso":
            externas_publicadas += 1
        episodios.append({"agente": nombre, **resultado})
    return {
        "episodios": episodios,
        "invalidos_aceptados": invalidos_aceptados,
        "escrituras_externas_publicadas": externas_publicadas,
        "etiquetas_inconsistentes": etiquetas_inconsistentes,
    }


def _confirmar_por_terminal(resumen: dict) -> bool:
    print(json.dumps(resumen, ensure_ascii=False, indent=2), file=sys.stderr)
    return input("¿Publicar este delta? [s/N] ").strip().lower() in {"s", "si", "sí"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--binary", default="target/debug/universo.exe" if os.name == "nt" else "target/debug/universo"
    )
    subcomandos = parser.add_subparsers(dest="comando", required=True)
    subcomandos.add_parser("demo", help="corre los cuatro agentes guionizados en un temporal")
    episodio = subcomandos.add_parser("episodio", help="verifica y publica un delta de un archivo")
    episodio.add_argument("repo", type=Path, help="directorio con bundle/, manifest.toml e invariants.sx")
    episodio.add_argument("--delta", type=Path, required=True)
    episodio.add_argument("--objetivo", required=True)
    episodio.add_argument("--registro", type=Path)
    episodio.add_argument("--confirmar", action="store_true", help="pide un sí explícito antes de aplicar")
    argumentos = parser.parse_args()
    binario = Path(argumentos.binary).resolve()
    if argumentos.comando == "demo":
        with tempfile.TemporaryDirectory(prefix="felix-orquestador-demo-") as nombre:
            print(json.dumps(demo(binario, Path(nombre)), ensure_ascii=False))
        return 0
    repo = Repositorio(
        argumentos.repo / "bundle", argumentos.repo / "manifest.toml", argumentos.repo / "invariants.sx"
    )
    try:
        validar_objetivo(argumentos.objetivo)
        delta = json.loads(argumentos.delta.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        print(json.dumps({"resultado": "escalado", "motivo": "error_tecnico", "intentos": 0}))
        return 1

    class _DesdeArchivo:
        def proponer(self, vista, retroalimentacion):
            return delta

    resultado = Orquestador(
        binario,
        repo,
        registro=argumentos.registro,
        presupuesto=1,
        confirmar=_confirmar_por_terminal if argumentos.confirmar else None,
    ).ejecutar(_DesdeArchivo(), argumentos.objetivo)
    print(json.dumps(resultado, ensure_ascii=False))
    return 0 if resultado["resultado"] == "publicado" else 1


if __name__ == "__main__":
    raise SystemExit(main())
