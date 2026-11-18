# Propuesta 08 — Prototipo de orquestador obligatorio

Estado: **aprobada e implementada como prototipo** (2026-11-18): [`tools/orquestador.py`](../../tools/orquestador.py), tests en [`tools/test_orquestador.py`](../../tools/test_orquestador.py). Corresponde al paso 8
del [maestro](../brechas/00-maestro.md), a G3/G4 de la
[brecha 04](../brechas/04-agentes-y-control-obligatorio.md) y a la prioridad 2
de las [conclusiones](../conclusiones-ia-felix-felicis.md#6-prioridades-y-decisión).

## Qué se quiere demostrar

Que un flujo determinista, y no la disciplina del modelo, impide que un cambio
inválido llegue al bundle, y que el agente no tiene otra ruta de escritura. La
hipótesis es de **control del flujo**, no de razonamiento: no se mide si el
modelo propone mejor.

## Diseño propuesto

```text
agente ──delta──▶ orquestador ──propose --explain──▶ universo (solo lectura)
   ▲                  │  rechazo: diagnóstico acotado + presupuesto
   └──────────────────┘
                      │  admit sobre snapshot S
                      ▼
              ejecutor autorizado ──propose --apply --expect-snapshot S──▶ bundle
                      │
                      ▼
               universo guard (sello)  ──▶ veredicto final del episodio
```

1. **Aislamiento.** El agente recibe una copia desechable del repositorio (lo
   que escriba ahí se descarta) y devuelve su delta como valor; no recibe la
   ruta real ni la del buzón, que vive en otro temporal. El orquestador guarda
   una copia canónica del delta: lo que se verifica, se confirma y se aplica es
   ese mismo texto. Sólo el ejecutor llama a `--apply`. No hay permisos del
   sistema operativo de por medio (ver límites).
2. **Verificación determinista.** Para cada delta, el orquestador corre
   `universo propose --explain decide --rules` (id y mensaje de regla en el
   JSON; `null` si la regla no los declara). No
   depende de que el agente decida consultar.
3. **Reparación con presupuesto.** Ante un rechazo, devuelve al agente el objeto
   de explicación ya redactado y un contador de intentos (por defecto 3). Ante
   error técnico, timeout o presupuesto agotado, el episodio termina en
   `escalado`; nunca se interpreta como admisión.
4. **Publicación.** Sólo con `admit` el ejecutor aplica con CAS sobre el
   snapshot verificado y vuelve a sellar. Si el snapshot cambió entre la
   verificación y el apply (por ejemplo, otra publicación legítima), el CAS
   falla y el episodio termina en `escalado` (`snapshot_cambiado`); no se
   reintenta en silencio.
5. **Auditoría.** Cada episodio deja un registro JSONL: hash de cada delta,
   veredictos, ids de regla, intentos y resultado, incluido el escalado cuando
   `guard` detecta escrituras fuera del ejecutor. La métrica principal es «cambios inválidos aceptados = 0» con
   «rutas de escritura alternativas = 0».

## Alcance del prototipo

- Agentes **guionizados y deterministas** (sin LLM), como `tools/demo_mcp.py`:
  uno correcto, uno que repara tras un rechazo, uno que agota el presupuesto y
  uno que intenta escribir el Markdown directamente (debe quedar detectado y
  sin publicar).
- Implementado en Python en `tools/orquestador.py`, sobre la CLI existente; no
  cambia el motor, el MCP ni sus contratos.
- Tests en CI (Linux y Windows) como el resto de `tools/`.

## Fuera de alcance

- No se conecta a `run_paired.py` ni toca el piloto 173: eso es el paso 9
  (estudio de cuatro brazos), que tiene su propia compuerta.
- No hay `apply` por MCP ni reparación sugerida automática (paso 10).
- No se usa un LLM real.

## Uso

```sh
python tools/orquestador.py demo
python tools/orquestador.py episodio <repo> --delta d.json --objetivo "..." [--confirmar] [--registro r.jsonl]
```

`<repo>` es un directorio como el que crea `universo init` (`bundle/`,
`manifest.toml`, `invariants.sx`). `episodio` usa un solo intento: el delta
viene de un archivo y no hay agente que repare.

Resultados: `publicado`, `escalado`, `no_confirmado` e `incierto`
(`apply_sin_confirmar`): el apply se lanzó y su salida no permite descartar un
commit. Sólo `stale` (el CAS falló antes de preparar) y `reject` (la
reevaluación bajo lock rechazó antes de instalar) garantizan que el bundle no
se tocó; cualquier otra salida —timeout, salida ilegible, errores
`*_unverified` con la transacción ya iniciada, `admit` con código de salida
distinto de 0 o un código desconocido— es `incierto`. Revisa con
`universo status`, `recover` y `guard`. Si el apply ya hizo commit, el resultado es
`publicado` aunque lo que sigue falle; esos fallos van en `alertas`
(`sin_sello`, `guard_final_fallido`, `error_interno`, `registro_incompleto`).
Motivos de escalado: `estado_inicial_no_sellado`, `escritura_externa`,
`transaccion_pendiente`, `error_agente`, `error_tecnico`, `error_interno`,
`presupuesto_agotado`, `snapshot_cambiado` (el CAS devolvió `stale`) y
`rechazo_al_aplicar`.

El registro JSONL guarda, por evento, sólo claves fijas: huellas HMAC del
objetivo y de cada delta (con una clave aleatoria por orquestador, para
correlacionar intentos sin poder confirmar un contenido adivinado),
veredictos, ids de regla, conteos y el resultado. El **objetivo** es texto
libre que sí queda en claro en el historial de Felix (`universo log`, vía
`--reason`) y en los argumentos del proceso: no escribas ahí datos sensibles.
Debe ser de una línea, imprimible y de 1 a 500 caracteres.

La demo mide el bundle real y no la etiqueta: mira si los átomos (`universo
atomos`) contienen el efecto del delta válido, comprueba invariantes con
`check` y reporta
`invalidos_aceptados`, `escrituras_externas_publicadas` y
`etiquetas_inconsistentes` (etiqueta `publicado` sin commit, o al revés).

## Límites del prototipo

- El aislamiento es por construcción: el agente corre en el mismo proceso y
  no recibe la ruta real, pero ningún sandbox del sistema operativo se lo
  impide. Una fuga de ruta queda **detectada** por `guard` (agente `intruso`),
  no prohibida.
- `guard` es una foto sin lock. Una escritura externa entre el último `guard`
  y el apply la frena el CAS (`snapshot_cambiado`). Una escritura posterior al
  apply la ve el `guard` final como alerta `guard_final_fallido`, pero el
  resultado sigue siendo `publicado`, porque el commit ya ocurrió.
- Los errores de forma del delta (por ejemplo `add_conflict`) escalan como
  `error_tecnico` en vez de devolverse para reparar.

## Decisiones tomadas

Aprobadas por Romeo el 2026-11-18 tal como se recomendaron: publicación
automática tras `admit` con `--confirmar` opcional, sólo agentes guionizados y
presupuesto de 3 intentos.

## Decisiones que necesitaba

1. **Autoridad de publicación (G3):** ¿el ejecutor aplica automáticamente tras
   `admit` o exige confirmación humana? Recomendación: automática dentro del
   harness aislado, con un modo `--confirmar` que pide un sí explícito antes del
   apply.
2. **Agentes del prototipo:** ¿sólo guionizados o también un LLM real?
   Recomendación: sólo guionizados; el LLM real pertenece al estudio del paso 9.
3. **Presupuesto de reparación:** recomendación de 3 intentos por episodio.
