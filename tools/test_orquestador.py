"""Tests del prototipo de orquestador obligatorio (paso 8)."""

from __future__ import annotations

import json
import os
from pathlib import Path
import tempfile
import unittest

import orquestador as orq

RAIZ_REPO = Path(__file__).resolve().parent.parent
BINARIO = Path(
    os.environ.get(
        "UNIVERSO",
        RAIZ_REPO / "target" / "debug" / ("universo.exe" if os.name == "nt" else "universo"),
    )
)

VALIDO = {
    "schema": "felix-delta-v1",
    "add": ["Estado(notas/bienvenida, vigente)"],
    "remove": ["Estado(notas/bienvenida, borrador)"],
}
# Viola «vigente-es-criterio»: una fuente no puede estar vigente.
INVALIDO = {
    "schema": "felix-delta-v1",
    "add": ["Type(notas/bienvenida, fuente)", "Estado(notas/bienvenida, vigente)"],
    "remove": ["Type(notas/bienvenida, criterio)", "Estado(notas/bienvenida, borrador)"],
}


class Guionizado:
    """Devuelve una secuencia fija de deltas y guarda lo que recibió."""

    def __init__(self, *deltas):
        self.deltas = list(deltas)
        self.recibido = []

    def proponer(self, vista, retroalimentacion):
        self.recibido.append((vista, retroalimentacion))
        return self.deltas[min(len(self.recibido), len(self.deltas)) - 1]


class OrquestadorTests(unittest.TestCase):
    def setUp(self):
        if not BINARIO.is_file():
            self.fail(f"falta el binario {BINARIO}: compila con `cargo build -p universo-cli`")
        self._tmp = tempfile.TemporaryDirectory(prefix="felix-orq-test-")
        self.base = Path(self._tmp.name)
        self.repo = orq.crear_repositorio_demo(BINARIO, self.base / "kb")
        self.registro = self.base / "registro.jsonl"

    def tearDown(self):
        self._tmp.cleanup()

    def orquestador(self, **opciones):
        return orq.Orquestador(BINARIO, self.repo, registro=self.registro, **opciones)

    def bytes_bundle(self):
        return {
            p.relative_to(self.repo.raiz).as_posix(): p.read_bytes()
            for p in sorted(self.repo.raiz.rglob("*"))
            if p.is_file()
        }

    def eventos(self):
        return [json.loads(linea) for linea in self.registro.read_text(encoding="utf-8").splitlines()]

    def test_delta_valido_se_publica_y_queda_sellado(self):
        resultado = self.orquestador().ejecutar(Guionizado(VALIDO), "activar bienvenida")
        self.assertEqual(resultado["resultado"], "publicado")
        self.assertEqual(resultado["intentos"], 1)
        self.assertIn("estado: vigente", (self.repo.raiz / "notas" / "bienvenida.md").read_text(encoding="utf-8"))
        self.assertTrue(orq.guardia(BINARIO, self.repo)["ok"])

    def test_rechazo_se_devuelve_redactado_y_el_agente_repara(self):
        agente = Guionizado(INVALIDO, VALIDO)
        resultado = self.orquestador().ejecutar(agente, "activar bienvenida")
        self.assertEqual(resultado["resultado"], "publicado")
        self.assertEqual(resultado["intentos"], 2)
        primera, segunda = agente.recibido
        self.assertIsNone(primera[1])
        retro = segunda[1]
        self.assertEqual(retro["veredicto"], "reject")
        self.assertEqual(retro["intentos_restantes"], 2)
        self.assertEqual([o["rule_id"] for o in retro["objetos"]], ["vigente-es-criterio"])

    def test_presupuesto_agotado_escala_sin_tocar_el_bundle(self):
        antes = self.bytes_bundle()
        resultado = self.orquestador(presupuesto=3).ejecutar(Guionizado(INVALIDO), "x")
        self.assertEqual(resultado["resultado"], "escalado")
        self.assertEqual(resultado["motivo"], "presupuesto_agotado")
        self.assertEqual(resultado["intentos"], 3)
        self.assertEqual(self.bytes_bundle(), antes)

    def test_escritura_fuera_del_ejecutor_se_detecta_y_no_se_publica(self):
        nota = self.repo.raiz / "notas" / "bienvenida.md"

        class Intruso:
            def proponer(self, vista, retroalimentacion):
                # Escribe el Markdown real por una ruta filtrada y luego
                # propone un delta válido para parecer legítimo.
                nota.write_text(nota.read_text(encoding="utf-8") + "\nintruso\n", encoding="utf-8")
                return VALIDO

        resultado = self.orquestador().ejecutar(Intruso(), "x")
        self.assertEqual(resultado["resultado"], "escalado")
        self.assertEqual(resultado["motivo"], "escritura_externa")
        self.assertIn("estado: borrador", nota.read_text(encoding="utf-8"))
        self.assertNotIn("publicacion", [e["evento"] for e in self.eventos()])

    def test_la_vista_es_una_copia_desechable(self):
        class Vandalo:
            def proponer(self, vista, retroalimentacion):
                for archivo in vista.rglob("*.md"):
                    archivo.write_text("roto", encoding="utf-8")
                return VALIDO

        resultado = self.orquestador().ejecutar(Vandalo(), "x")
        self.assertEqual(resultado["resultado"], "publicado")
        self.assertIn("estado: vigente", (self.repo.raiz / "notas" / "bienvenida.md").read_text(encoding="utf-8"))

    def test_estado_inicial_no_sellado_escala_antes_de_preguntar(self):
        nota = self.repo.raiz / "notas" / "bienvenida.md"
        nota.write_text(nota.read_text(encoding="utf-8") + "\nmanual\n", encoding="utf-8")
        agente = Guionizado(VALIDO)
        resultado = self.orquestador().ejecutar(agente, "x")
        self.assertEqual((resultado["resultado"], resultado["motivo"]), ("escalado", "estado_inicial_no_sellado"))
        self.assertEqual(agente.recibido, [])

    def test_delta_mal_formado_o_error_tecnico_nunca_es_admision(self):
        for malo in ({"schema": "felix-delta-v1", "add": ["NoExiste(x)"], "remove": []}, "no es un objeto", None):
            with self.subTest(malo=malo):
                antes = self.bytes_bundle()
                resultado = self.orquestador().ejecutar(Guionizado(malo), "x")
                self.assertEqual((resultado["resultado"], resultado["motivo"]), ("escalado", "error_tecnico"))
                self.assertEqual(self.bytes_bundle(), antes)

    def test_error_de_forma_del_delta_se_devuelve_para_reparar_si_se_pide(self):
        # Retirar algo que no existe es `remove_not_found`, un error de forma.
        mal_formado = {"schema": "felix-delta-v1", "add": [], "remove": ["Estado(notas/bienvenida, vigente)"]}
        sin_opcion = self.orquestador().ejecutar(Guionizado(mal_formado, VALIDO), "x")
        self.assertEqual((sin_opcion["resultado"], sin_opcion["motivo"]), ("escalado", "error_tecnico"))
        self.repo = orq.crear_repositorio_demo(BINARIO, self.base / "kb-con-opcion")
        agente = Guionizado(mal_formado, VALIDO)
        con_opcion = self.orquestador(reparar_errores_de_delta=True).ejecutar(agente, "x")
        self.assertEqual((con_opcion["resultado"], con_opcion["intentos"]), ("publicado", 2))
        retro = agente.recibido[1][1]
        self.assertEqual((retro["veredicto"], retro["codigos"], retro["objetos"]), ("error", ["remove_not_found"], []))

    def test_un_conflicto_funcional_es_un_rechazo_reparable(self):
        # Reemplazar un valor de un solo valor sin retirar el anterior.
        conflicto = {"schema": "felix-delta-v1", "add": ["Estado(notas/bienvenida, vigente)"], "remove": []}
        agente = Guionizado(conflicto, VALIDO)
        resultado = self.orquestador().ejecutar(agente, "x")
        self.assertEqual((resultado["resultado"], resultado["intentos"]), ("publicado", 2))
        retro = agente.recibido[1][1]
        self.assertEqual((retro["veredicto"], retro["conflictos"], retro["objetos"]), ("reject", 1, []))
        # Un error del entorno nunca se devuelve como reparable.
        for tecnico in ("snapshot_bundle_invalid", "prepared_installation_unverified", "writer_lock_busy",
                        "stage_io", "stale", "instance_budget_exhausted", "replay_mismatch", "algo_nuevo"):
            self.assertFalse(orq._es_error_de_delta(tecnico), tecnico)
        self.assertTrue(orq._es_error_de_delta("label_missing"))

    def test_formas_y_metadatos_de_reglas(self):
        texto = '; comentario (no)\n(regla :id "a-1" :mensaje "Con \\"comillas\\" y (paréntesis)." (Type @n/x a))\n(Type @n/y a)\n'
        formas = orq.formas_de_nivel_superior(texto)
        self.assertEqual(len(formas), 2)
        self.assertEqual(orq.metadatos_de_regla(formas[0]), ("a-1", 'Con "comillas" y (paréntesis).'))
        self.assertEqual(orq.metadatos_de_regla(formas[1]), (None, None))
        self.assertEqual(orq.metadatos_de_regla('(regla :mensaje "Con (p)\nlínea" :id "x1" (Type @n/x a))'),
                         ("x1", "Con (p)\nlínea"))
        self.assertEqual(orq.metadatos_de_regla('(regla-x :id "no" (Type @n/x a))'), (None, None))
        with self.assertRaises(ValueError):
            orq.formas_de_nivel_superior("(a (b)")

    def test_explicacion_sin_presupuesto_se_reemplaza_por_simulacion_por_regla(self):
        real = orq._universo

        def sin_presupuesto(binario, argumentos):
            codigo, salida = real(binario, argumentos)
            if "--explain" in argumentos and salida.get("status") == "reject":
                salida = {**salida, "explanations": None, "explanation_error": "explanation_budget_exceeded"}
            return codigo, salida

        agente = Guionizado(INVALIDO, VALIDO)
        orq._universo = sin_presupuesto
        try:
            resultado = self.orquestador().ejecutar(agente, "x")
        finally:
            orq._universo = real
        self.assertEqual(resultado["resultado"], "publicado")
        objetos = agente.recibido[1][1]["objetos"]
        self.assertEqual([(o["rule_id"], o["kind"]) for o in objetos], [("vigente-es-criterio", "rule")])

    def test_con_testigo_el_rechazo_dice_donde(self):
        agente = Guionizado(INVALIDO, VALIDO)
        resultado = self.orquestador(testigo=True).ejecutar(agente, "x")
        self.assertEqual(resultado["resultado"], "publicado")
        testigo = agente.recibido[1][1]["testigo"]
        self.assertEqual(testigo["rule_id"], "vigente-es-criterio")
        self.assertEqual(testigo["bindings"], [{"variable": "x", "kind": "node", "value": "notas/bienvenida"}])
        # Sin la opción no viajan rutas.
        self.repo = orq.crear_repositorio_demo(BINARIO, self.base / "kb-sin-testigo")
        agente = Guionizado(INVALIDO, VALIDO)
        self.orquestador().ejecutar(agente, "x")
        self.assertIsNone(agente.recibido[1][1]["testigo"])

    def test_excepcion_del_agente_escala(self):
        class Roto:
            def proponer(self, vista, retroalimentacion):
                raise RuntimeError("fallo del modelo")

        resultado = self.orquestador().ejecutar(Roto(), "x")
        self.assertEqual((resultado["resultado"], resultado["motivo"]), ("escalado", "error_agente"))

    def test_confirmacion_humana_negada_no_publica(self):
        preguntas = []
        antes = self.bytes_bundle()
        resultado = self.orquestador(confirmar=lambda resumen: preguntas.append(resumen) or False).ejecutar(
            Guionizado(VALIDO), "activar bienvenida"
        )
        self.assertEqual(resultado["resultado"], "no_confirmado")
        self.assertEqual(len(preguntas), 1)
        self.assertEqual(preguntas[0]["objetivo"], "activar bienvenida")
        self.assertEqual(self.bytes_bundle(), antes)
        resultado = self.orquestador(confirmar=lambda resumen: True).ejecutar(Guionizado(VALIDO), "x")
        self.assertEqual(resultado["resultado"], "publicado")

    def test_publicacion_concurrente_legitima_hace_fallar_el_cas(self):
        # Otra publicación por Felix entre la verificación y el apply deja el
        # estado sellado (guard pasa), pero el snapshot ya no es el verificado.
        otro = {
            "schema": "felix-delta-v1",
            "add": ['HasLab(notas/bienvenida, "Hola")'],
            "remove": ['HasLab(notas/bienvenida, "Bienvenida")'],
        }

        def publicar_otro(resumen):
            self.assertEqual(orq.Orquestador(BINARIO, self.repo).ejecutar(Guionizado(otro), "otro")["resultado"], "publicado")
            return True

        resultado = self.orquestador(confirmar=publicar_otro).ejecutar(Guionizado(VALIDO), "x")
        self.assertEqual((resultado["resultado"], resultado["motivo"]), ("escalado", "snapshot_cambiado"))
        texto = (self.repo.raiz / "notas" / "bienvenida.md").read_text(encoding="utf-8")
        self.assertIn("title: Hola", texto)
        self.assertIn("estado: borrador", texto)

    def test_registro_jsonl_solo_tiene_claves_permitidas(self):
        self.orquestador().ejecutar(Guionizado(INVALIDO, VALIDO), "activar bienvenida")
        eventos = self.eventos()
        self.assertEqual([e["evento"] for e in eventos], ["inicio", "veredicto", "veredicto", "publicacion", "fin"])
        self.assertTrue(all(e["episodio"] == eventos[0]["episodio"] for e in eventos))
        permitidas = {
            "inicio": {"episodio", "evento", "objetivo_hmac", "presupuesto"},
            "veredicto": {"episodio", "evento", "intento", "delta_hmac", "veredicto", "reglas", "codigos", "invariantes_fallidas"},
            "publicacion": {"episodio", "evento", "sellado"},
            "fin": {"episodio", "evento", "resultado", "motivo", "intentos", "alertas"},
        }
        for evento in eventos:
            self.assertEqual(set(evento), permitidas[evento["evento"]], evento["evento"])
        self.assertEqual(eventos[1]["reglas"], ["vigente-es-criterio"])
        self.assertEqual(len(eventos[1]["delta_hmac"]), 64)
        # Con otra clave el mismo delta da otra huella: no se puede confirmar
        # un candidato recalculando el hash.
        self.assertNotEqual(eventos[2]["delta_hmac"], orq.Orquestador(BINARIO, self.repo)._huella(VALIDO))
        texto = self.registro.read_text(encoding="utf-8")
        # `vigente` sí aparece, como parte del id de regla `vigente-es-criterio`.
        for sensible in ("bienvenida", "Bienvenida", "borrador", "Sólo un criterio", "activar"):
            self.assertNotIn(sensible, texto)

    def test_cada_camino_termina_con_evento_fin(self):
        class Roto:
            def proponer(self, vista, retroalimentacion):
                raise RuntimeError

        for agente in (Guionizado(VALIDO), Guionizado(INVALIDO), Roto(), Guionizado({"a": {1}})):
            self.registro.unlink(missing_ok=True)
            self.repo = orq.crear_repositorio_demo(BINARIO, self.base / f"kb-{id(agente)}")
            resultado = self.orquestador().ejecutar(agente, "x")
            eventos = self.eventos()
            self.assertEqual(eventos[-1]["evento"], "fin")
            self.assertEqual(eventos[-1]["resultado"], resultado["resultado"])

    def test_delta_no_serializable_escala_como_error_tecnico(self):
        resultado = self.orquestador().ejecutar(Guionizado({"schema": {1, 2}}), "x")
        self.assertEqual((resultado["resultado"], resultado["motivo"]), ("escalado", "error_tecnico"))

    def test_el_agente_no_ve_la_ruta_real_ni_el_buzon(self):
        agente = Guionizado(VALIDO)
        self.orquestador().ejecutar(agente, "x")
        vista = agente.recibido[0][0]
        self.assertFalse(vista.resolve().is_relative_to(self.repo.raiz.parent.resolve()))
        self.assertFalse((vista.parent / "buzon").exists() or (vista.parent.parent / "buzon").exists())

    def test_objetivo_invalido_se_rechaza_antes_de_empezar(self):
        agente = Guionizado(VALIDO)
        for malo in ("", "   ", "a\nb", "x" * 501, "\u202eoculto", 7):
            with self.subTest(malo=malo), self.assertRaises(ValueError):
                self.orquestador().ejecutar(agente, malo)
        self.assertEqual(agente.recibido, [])
        # Un guion inicial es válido: va como `--reason=-…`.
        self.assertEqual(self.orquestador().ejecutar(agente, "-arreglo")["resultado"], "publicado")

    def test_confirmacion_que_falla_o_no_es_true_no_autoriza(self):
        def explota(resumen):
            raise RuntimeError

        for confirmar in (explota, lambda r: "sí", lambda r: 1):
            with self.subTest(confirmar=confirmar):
                antes = self.bytes_bundle()
                resultado = self.orquestador(confirmar=confirmar).ejecutar(Guionizado(VALIDO), "x")
                self.assertEqual(resultado["resultado"], "no_confirmado")
                self.assertEqual(self.bytes_bundle(), antes)

    def test_escritura_externa_justo_antes_del_apply_la_frena_el_cas(self):
        nota = self.repo.raiz / "notas" / "bienvenida.md"

        class Carrera(orq.Orquestador):
            def _aplicar(self, archivo, token, objetivo):
                nota.write_text(nota.read_text(encoding="utf-8") + "\ncarrera\n", encoding="utf-8")
                return super()._aplicar(archivo, token, objetivo)

        resultado = Carrera(BINARIO, self.repo, registro=self.registro).ejecutar(Guionizado(VALIDO), "x")
        self.assertEqual((resultado["resultado"], resultado["motivo"]), ("escalado", "snapshot_cambiado"))
        self.assertIn("estado: borrador", nota.read_text(encoding="utf-8"))
        self.assertNotIn("publicacion", [e["evento"] for e in self.eventos()])

    def test_publicado_con_fallo_posterior_sigue_siendo_publicado(self):
        nota = self.repo.raiz / "notas" / "bienvenida.md"

        class Despues(orq.Orquestador):
            def _aplicar(self, archivo, token, objetivo):
                salida = super()._aplicar(archivo, token, objetivo)
                nota.write_text(nota.read_text(encoding="utf-8") + "\ndespues\n", encoding="utf-8")
                return salida

        resultado = Despues(BINARIO, self.repo, registro=self.registro).ejecutar(Guionizado(VALIDO), "x")
        self.assertEqual(resultado["resultado"], "publicado")
        self.assertEqual(resultado["alertas"], ["guard_final_fallido"])

    def test_apply_ambiguo_es_incierto_y_un_fallo_interno_tras_publicar_no_lo_oculta(self):
        class Timeout(orq.Orquestador):
            def _aplicar(self, archivo, token, objetivo):
                orq.TIEMPO_LIMITE_S, previo = 0.001, orq.TIEMPO_LIMITE_S
                try:
                    return super()._aplicar(archivo, token, objetivo)
                finally:
                    orq.TIEMPO_LIMITE_S = previo

        resultado = Timeout(BINARIO, self.repo).ejecutar(Guionizado(VALIDO), "x")
        self.assertEqual((resultado["resultado"], resultado["motivo"]), ("incierto", "apply_sin_confirmar"))

        repo = orq.crear_repositorio_demo(BINARIO, self.base / "kb2")

        class FalloInterno(orq.Orquestador):
            def _anotar(self, episodio, evento, **campos):
                if evento == "publicacion":
                    raise KeyError("fallo interno tras el commit")
                super()._anotar(episodio, evento, **campos)

        resultado = FalloInterno(BINARIO, repo).ejecutar(Guionizado(VALIDO), "x")
        self.assertEqual((resultado["resultado"], resultado["alertas"]), ("publicado", ["error_interno"]))

    def test_salidas_del_apply_que_no_descartan_un_commit_son_inciertas(self):
        real = orq._universo
        for falsa, esperado in [
            ((1, {"status": "error", "errors": [{"code": "prepared_installation_unverified"}]}), ("incierto", "apply_sin_confirmar")),
            ((1, {"status": "error", "errors": [{"code": "apply_recovery_unverified"}]}), ("incierto", "apply_sin_confirmar")),
            ((1, {"status": "admit", "mode": "apply"}), ("incierto", "apply_sin_confirmar")),
            ((1, {"status": "error", "errors": [{"code": "algo_nuevo"}]}), ("incierto", "apply_sin_confirmar")),
            ((1, {"status": "error", "errors": [{"code": "stale"}]}), ("escalado", "snapshot_cambiado")),
            ((1, {"status": "reject", "mode": "apply"}), ("escalado", "rechazo_al_aplicar")),
        ]:
            with self.subTest(falsa=falsa):
                def universo(binario, argumentos, falsa=falsa):
                    return falsa if "--apply" in argumentos else real(binario, argumentos)

                orq._universo = universo
                try:
                    resultado = self.orquestador().ejecutar(Guionizado(VALIDO), "x")
                finally:
                    orq._universo = real
                self.assertEqual((resultado["resultado"], resultado["motivo"]), esperado)

    def test_timeout_o_salida_ilegible_al_verificar_escala(self):
        class Lento(orq.Orquestador):
            def _simular(self, archivo):
                orq.TIEMPO_LIMITE_S, previo = 0.001, orq.TIEMPO_LIMITE_S
                try:
                    return super()._simular(archivo)
                finally:
                    orq.TIEMPO_LIMITE_S = previo

        antes = self.bytes_bundle()
        resultado = Lento(BINARIO, self.repo).ejecutar(Guionizado(VALIDO), "x")
        self.assertEqual((resultado["resultado"], resultado["motivo"]), ("escalado", "error_tecnico"))
        self.assertEqual(self.bytes_bundle(), antes)

    def test_presupuesto_invalido(self):
        with self.assertRaises(ValueError):
            self.orquestador(presupuesto=0)

    def test_demo_completa_cero_invalidos_aceptados(self):
        resumen = orq.demo(BINARIO, self.base / "demo")
        self.assertEqual(
            {e["agente"]: e["resultado"] for e in resumen["episodios"]},
            {"correcto": "publicado", "reparador": "publicado", "obstinado": "escalado", "intruso": "escalado"},
        )
        self.assertEqual(resumen["invalidos_aceptados"], 0)
        self.assertEqual(resumen["escrituras_externas_publicadas"], 0)
        self.assertEqual(resumen["etiquetas_inconsistentes"], 0)


if __name__ == "__main__":
    unittest.main()
