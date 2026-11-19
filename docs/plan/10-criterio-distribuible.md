# 10 — Criterio distribuible: la herramienta aquí, el criterio en su propio repositorio

El objetivo es que Felix sirva en local, para ti y para cualquier agente, como
**fuente de criterio**: no un almacén de datos, sino de decisiones, principios
y preferencias que se verifican antes de aceptarse. El criterio vive en un
repositorio Git propio y privado, separado de este. Así se puede llevar a otro
computador y compartir sólo con quien se invite.

Esta es la fase 10 del [plan maestro](00-plan-maestro.md). No es el «paso 10»
de las [conclusiones](../conclusiones-ia-felix-felicis.md) (reparación
sugerida, revisión AGM, recuperación avanzada), que sigue fuera de alcance.

## Estado

| Campo | Valor |
|---|---|
| Estado | en curso (2026-11-18): plan aceptado con las recomendaciones de D1 a D10; paso 1 (U1, U2) hecho, salvo el argumento `universe` por llamada del MCP y `search --universo todos`, que pasan a U6 |
| Depende de | `v0.3.0` (búsqueda, historial con efecto, `diff`, instalación) |
| Desbloquea | Uso diario real, con un repositorio de criterio propio que reemplaza a la instancia privada legacy |

## Cómo nace el criterio

El criterio no se dicta en un formulario: aparece mientras se trabaja con un
agente. La persona dice cómo quiere las cosas («me gusta que los componentes
sean 100 % reutilizables», «antes de crear algo, valida que no exista; si
existe, reutilízalo»). En algún momento lo valida: lo dice («quedó bien») o lo
da a entender pasando a otro tema sin objetar.

El trabajo de Felix es que esas afirmaciones no se pierdan, y que las que se
sostienen en el tiempo ganen peso. Hay tres momentos:

1. **Detectar y capturar.** Un agente reconoce una instrucción que expresa
   criterio y la guarda como **candidata**. Guarda una paráfrasis concisa y
   el contexto en que se dijo: proyecto, tarea, fecha, y si se validó de forma
   explícita o implícita. Ese contexto es lo que la hace verdadera en un
   ámbito, y sin él la frase pierde sentido.
2. **Reforzar.** Cuando la misma premisa se aplica en otra conversación u otro
   proyecto y se sostiene, el agente registra esa evidencia. Si la persona la
   contradice, también queda registrado.
3. **Promover.** Una candidata con evidencia suficiente pasa a un universo de
   más calidad. Allí las reglas son más exigentes y la promoción la confirma
   la persona.

### Toda base empieza vacía

Quien instale Felix, tú incluido, empieza sin criterio. Al principio casi todo
el uso será capturar. Si capturar cuesta, la base no crece y la herramienta
nunca llega a ser útil. Las reglas llegan después, cuando el criterio se
repite y se ve qué debe cumplirse siempre. El valor crece con el uso, y hay
que hacerlo visible para saber cuándo la base empieza a pagar.

## Varios universos

Un repositorio de criterio no tiene un solo universo. Tiene varios bundles,
cada uno con su manifiesto y sus reglas, y la exigencia sube de uno a otro:

| Universo | Qué entra | Quién escribe | Reglas (ejemplos) |
|---|---|---|---|
| `candidatos` | Toda afirmación detectada, con su contexto | Cualquier agente | Tiene paráfrasis, proyecto, fecha y tipo de validación |
| `vigente` | Criterio probado | Sólo por promoción, confirmada por la persona | Confirmado en al menos dos proyectos; con contexto; sin `contradice` con otro vigente; lo reemplazado queda retirado |

Son los dos de partida; puede haber más. «De mejor calidad» tiene así un
sentido preciso: **reglas más estrictas que Felix verifica**, no una etiqueta.

### Una escalera hasta los axiomas

Los universos forman una escalera. Cada escalón exige más para entrar y
cambia menos:

```text
candidatos → vigente → consolidado → axiomas
```

| Escalón | Para entrar (ejemplos) | Cómo cambia |
|---|---|---|
| `consolidado` | vigente por un tiempo mínimo, confirmado en más proyectos, sin objeciones abiertas | rara vez |
| `axiomas` | consolidado, sin objeciones en todo su historial, y tu confirmación explícita y razonada | casi nunca; retirar un axioma es una decisión, no un refuerzo |

Los axiomas son pocos: lo que ya no se discute y desde donde se razona lo
demás.

Un axioma que se puede escribir como regla tiene además un segundo papel:
puede **bajar como invariante** a los universos inferiores con `reglas
aplicar` (U5). Desde entonces, nada que lo contradiga entra en `vigente` ni en
`consolidado`. Por ejemplo, «antes de crear algo, verifica si existe y
reutilízalo» se queda en prosa, porque guía al agente. En cambio, «toda
decisión vigente cita al menos una fuente» sí se puede verificar. Así el
criterio más probado termina gobernando la entrada del nuevo.

La escalera se configura en `felix.toml`: cuántos escalones, cómo se llaman y
qué reglas tiene cada uno. Nada obliga a usar los cuatro; se empieza con dos
(D7).

La regla de promoción ya se puede escribir con el motor actual. Verificado
sobre `v0.3.0`: `check` rechaza un criterio vigente confirmado en un solo
proyecto y admite el que tiene dos.

```text
(regla :id "vigente-confirmado" :mensaje "Un criterio vigente está confirmado en al menos dos proyectos."
  (forall ((x N))
    (=> (Estado x vigente)
        (al-menos 2 ((e E) (y N)) :guarda (Edge e x y confirmado_en) (Type y proyecto)))))
```

Lo que no se comparte con las mismas personas va en **otro repositorio**, no
en otro universo: el acceso de Git es por repositorio.

## Qué existe ya

Verificado sobre `v0.3.0`:

- `universo init <dir>` crea un directorio autocontenido: `bundle/` (notas en
  Markdown con frontmatter), `manifest.toml`, `invariants.sx` y el sello. Trae
  una plantilla mínima: tipos `criterio` y `fuente`, la relación `apoya` y el
  atributo `estado` (`borrador`, `vigente`).
- Tras un apply quedan junto al bundle:
  - **para versionar:** `.bundle.felix.log` (historial con delta, efecto y
    tokens) y `.bundle.felix.seal` (sello del último estado admitido);
  - **transitorios, fuera de Git:** `.bundle.felix.lock`, que queda vacío y
    permanente, y los de una transacción a medias (`*.felix.journal`,
    `*.felix.journal.tmp`, `*.felix.backup`, `*.felix.stage.*`,
    `*.felix.admctx*`).
- Todos los comandos piden `--manifiesto` y `--invariantes` con su ruta, y
  trabajan sobre un solo bundle. El MCP arranca con `--root`, `--manifiesto` y
  `--invariantes`.
- Crear una nota exige escribir un delta `felix-delta-v2` en JSON, o editar
  una copia del bundle y usar `universo delta --desde`.
- El MCP consulta, simula y explica, pero no escribe. Publicar pasa por la CLI
  (`propose --apply`) o por el orquestador.
- `seal` acepta el estado actual sin dejar entrada en el historial. Por eso,
  editar las reglas a mano y sellar deja un hueco que `diff` informa como
  `history_gap`.
- Hay un hook de pre-commit (`examples/guardia/pre-commit`) que corre `guard`.
- Con una nota en CRLF, `validate` sigue aceptando pero `guard` da `fail`
  porque el sello ya no coincide, y un cuerpo con `\r` que entra por un delta
  se rechaza (`note_invalid_body`).
- La instancia privada de la fase 08 queda como **legacy** desde el
  2026-11-18. No se migra ni sigue los tags nuevos, y nada de este plan
  depende de ella.

## Arquitectura

```text
felix_felicis (público, este repo)       mi-criterio (privado, tuyo)
───────────────────────────────────      ──────────────────────────────────
motor, CLI `universo`, MCP               felix.toml          ← universos, motor esperado
docs, ejemplos ficticios                 AGENTS.md           ← cuándo consultar, capturar, reforzar
releases con binarios                    README.md
                                         universos/
          instala ──────────────►          candidatos/      ← manifest.toml, invariants.sx,
          `universo` y `universo-mcp`                           bundle/, log y sello
                                           vigente/         ← ídem, con reglas más estrictas
                                         .gitignore  .gitattributes
```

- El motor no conoce dónde está ningún repositorio de criterio.
- El repositorio declara la versión de motor que espera, para que dos
  computadores con versiones distintas no lo interpreten distinto sin avisar.
- Git da copia, respaldo, historial y control de acceso. Felix no hace `push`
  ni `pull`.

## Unidades de trabajo

Cada unidad se hace con TDD, revisión independiente y CI verde, como las fases
anteriores.

### U1 — `felix.toml`: un repositorio que se describe solo

**Qué.** Un archivo en la raíz del repositorio de criterio:

```toml
schema = "felix-criterio-v1"
motor = ">=0.4, <0.5"            # versiones de universo compatibles
por_defecto = "candidatos"

[universos.candidatos]
dir = "universos/candidatos"     # contiene manifest.toml, invariants.sx y bundle/
escritura_agentes = true

[universos.vigente]
dir = "universos/vigente"
escritura_agentes = false
```

**Descubrimiento.** Todos los comandos resuelven el universo en este orden:

1. las opciones explícitas actuales (raíz, `--manifiesto`, `--invariantes`),
   que siguen funcionando igual;
2. `--criterio <dir>` y `--universo <nombre>`;
3. las variables `FELIX_CRITERIO` y `FELIX_UNIVERSO`;
4. el `felix.toml` más cercano, subiendo desde el directorio actual, con su
   universo `por_defecto`.

Así basta `universo search cache` dentro del repositorio, o
`universo search cache --universo vigente`. `search` acepta además
`--universo todos`, que busca en todos y marca de cuál viene cada resultado.

**Versión.** Si la versión de `universo` no cumple `motor`, el comando falla
con `engine_version_mismatch` y dice cuál pide. `--ignorar-version` lo salta,
de forma explícita.

**Aceptación.**
- [x] `check`, `search`, `log`, `show`, `diff`, `guard`, `propose` y `status`
      funcionan sin rutas dentro de un repositorio con `felix.toml` (también
      `atomos`, `validate`, `seal`, `recover` y `delta`).
- [x] La precedencia está probada: rutas explícitas, `--criterio`,
      `FELIX_CRITERIO` y búsqueda hacia arriba; `--universo` y `FELIX_UNIVERSO`.
- [x] Las rutas de `felix.toml` son relativas a él y no pueden salir del
      repositorio, tampoco por un enlace: error `criterio_path_invalid`.
- [x] Un universo inexistente es error (`universe_not_found`) con la lista de
      los que hay.
- [x] El MCP acepta `--criterio <dir> [--universo <n>]`.
- [ ] Sus herramientas reciben un argumento opcional `universe`; sin él usan
      `por_defecto` (aditivo en `felix-mcp-v1`). Queda para U6.
- [ ] `search --universo todos`. Queda para U6.
- Límites conocidos del paso 1:
  - `escritura_agentes` se lee pero todavía no se hace cumplir. Hasta U6
    es una convención, y `AGENTS.md` lo dice.
  - `show` y `diff` toman un primer argumento numérico como número de
    entrada, no como raíz. Un directorio llamado `2` se pasa como `./2`.
  - Un `FELIX_UNIVERSO` vacío se ignora (usa el universo por defecto); uno que
    no es texto es error de uso.

### U2 — `universo init --git`: un repositorio de criterio listo

**Qué.** `universo init <dir> --git` crea el repositorio con los dos
universos de partida y además:

- escribe `felix.toml`, `README.md` y `AGENTS.md` (U6);
- escribe un `.gitignore` con los transitorios listados arriba. Nunca ignora
  el historial ni el sello.
- escribe un `.gitattributes` con `* text eol=lf`, para que Git en Windows no
  convierta a CRLF. Además declara `merge=union` para los logs (U7).
- hace `git init` y deja el hook de pre-commit de `guard` instalado pero
  apagado. Se activa con una línea que el README indica.

No hace el primer commit ni configura un remoto (D3).

**Aceptación.**
- [x] `git status` tras `init --git` y un apply muestra sólo archivos que deben
      versionarse.
- [x] Un clon en otro directorio pasa `guard` en todos los universos, y el
      historial viaja con él.
- [x] Un clon con `core.autocrlf=true` también pasa `guard`; el test corre en
      Linux y en el job de Windows del CI.

### U3 — Capturar sin fricción, con contexto

Son comandos que arman el delta, lo simulan y lo aplican en un solo paso, con
CAS interno, autor y motivo. Por defecto escriben en `candidatos`:

```sh
universo capturar "Los componentes deben ser 100 % reutilizables" \
    --proyecto tienda-web --tarea "refactor del carrito" \
    --validacion explicita --autor agente:claude-code
universo nota editar notas/componentes-reutilizables    # abre $EDITOR y aplica
universo relacion notas/a reemplaza notas/b
universo atributo notas/a estado vigente
```

- `capturar` crea una nota candidata. La ruta sale del texto (minúsculas,
  sin tildes, guiones).
  - El texto es una **paráfrasis concisa**, no un fragmento de la
    conversación.
  - El contexto queda como atributos: `proyecto`, `fecha`, `tarea` y
    `validacion` (`explicita`, `implicita` o `pendiente`).
  - Si ya existe una candidata parecida (con `search`), avisa y propone
    reforzarla en vez de duplicarla (U12).
- `nota editar`, `relacion` y `atributo` sirven para cualquier universo.
  `nota editar` copia la nota a un borrador, abre el editor y, al cerrar, usa
  lo que ya hace `universo delta --desde`. Si no hubo cambios, no hay apply.
- Todos aceptan `--dry-run` y `--format json`. Un rechazo muestra la regla que
  falla.

**Aceptación.**
- [x] Capturar es un comando, sin escribir JSON (`universo capturar`).
- [x] Cada captura deja una entrada de historial con su efecto y su autor.
- [x] Un rechazo no modifica nada y muestra la regla que falla.
- [x] Una captura con el mismo título que un criterio existente (sin
      distinguir mayúsculas ni tildes) no duplica salvo con `--forzar`. No
      detecta paráfrasis: eso es búsqueda semántica, fuera de alcance.
- [x] `nota editar`, `relacion` y `atributo` (paso 3, 2026-11-18). `relacion
      --quitar` retira la de mayor ordinal; `atributo` reemplaza un valor único
      o suma y quita valores múltiples; `nota editar` usa `$VISUAL` o `$EDITOR`.
- Hecho en el paso 2 (2026-11-18):
  - **Esquema de captura v1**, el que deja `init --git` en cada universo: tipos
    `criterio`, `evidencia` y `proyecto`; aristas `confirmado_en`, `apoya`,
    `objeta` y `en_proyecto`; atributos `estado`, `validacion` y `fecha`.
    Los dominios de atributos son cerrados, así que el proyecto es una nota y
    la tarea y el contexto van en el cuerpo.
  - **La base empieza vacía:** sin nota de bienvenida.
  - **Notas con nombre único en el universo** (`proyecto-…`, `evidencia-…`),
    porque los enlaces del frontmatter se resuelven por nombre.

### U4 — Plantillas de los universos de partida

Plantillas para `candidatos` y `vigente`, opcionales y modificables con U5
(D2):

| Elemento | Propuesta |
|---|---|
| Tipos | `principio`, `decision`, `preferencia`, `antipatron`, `fuente`, `proyecto`, `evidencia` |
| Relaciones | `apoya` (evidencia o fuente → criterio), `objeta` (evidencia → criterio), `confirmado_en` (criterio → proyecto), `reemplaza` (nuevo → anterior), `contradice`, `aplica_a` |
| Atributos | `estado` (`candidato`, `vigente`, `retirado`), `validacion`, `confianza` (`baja` < `media` < `alta`), `fecha` y `vigente_desde` (fechas), `ambito` (varios valores) |
| Reglas en `candidatos` | toda candidata tiene `proyecto`, `fecha` y `validacion` |
| Reglas en `vigente` | confirmado en al menos dos proyectos (la regla de arriba); sin `contradice` entre dos vigentes; lo reemplazado queda retirado; un retirado tiene quien lo reemplace |

**Aceptación.**
- [ ] Ambas plantillas pasan `check` con una nota de ejemplo de cada tipo.
- [ ] Cada regla tiene un test que la hace fallar y otro que la cumple.

### U5 — Cambiar reglas y manifiesto sin romper el historial

**Qué.** `universo reglas aplicar <nuevo.sx>` y `universo manifiesto aplicar
<nuevo.toml>`, por universo:

1. toman el lock y comprueban el estado admitido (CAS);
2. verifican el bundle con la configuración nueva. Si algo incumple una regla
   nueva, rechazan y dicen qué;
3. instalan el archivo nuevo;
4. registran una entrada de historial de clase `config` con autor, motivo,
   tokens `from` y `to`, y el efecto.

Además, `seal` pasa a registrar una entrada de clase `seal` (cambio externo
aceptado), con `from` y `to`. Así la cadena no se rompe: `diff` a través de
una entrada `config` marca «reglas cambiadas», y a través de una `seal`
responde `external_change` con la entrada.

**Aceptación.**
- [ ] Endurecer una regla que el bundle incumple se rechaza sin escribir.
- [ ] Tras `reglas aplicar`, `diff` sobre el rango funciona y lo marca.
- [ ] `seal` de una edición externa deja la entrada `seal`, y `diff` la
      informa por su nombre.
- [ ] Los lectores anteriores de `felix-log-v1` siguen leyendo el log.

### U6 — Que cualquier agente lo use

- **`AGENTS.md`**, generado por `init`, explica:
  - cuándo consultar (antes de decidir, con `search` y `context` en
    `vigente` y luego en `candidatos`);
  - cuándo capturar, reforzar u objetar (U13);
  - qué no hacer: no promover, no editar archivos a mano, no sellar.
- **`universo mcp-config --cliente claude-code|codex|json`** imprime la
  configuración lista para pegar, con rutas absolutas al clon local. Para
  Claude Code recomienda registrarlo a nivel de usuario (`claude mcp add
  --scope user ...`), porque el criterio se consulta desde cualquier proyecto.
- **Escritura por MCP: herramienta `capture`.** Escribe con CAS, autor
  `agente:<nombre>` y motivo obligatorio. Sólo existe para universos con
  `escritura_agentes = true`, es decir, `candidatos`. Nunca para `vigente`:
  allí el MCP sigue sin escribir, como en la fase 06. Lo mismo vale para
  `reforzar` y `objetar` (U12).

**Aceptación.**
- [ ] Con sólo `AGENTS.md` y la configuración MCP, un agente de prueba
      consulta, captura una candidata, la refuerza en otro proyecto y recibe un
      rechazo explicado al intentar algo inválido.
- [ ] `capture` sobre `vigente` no existe: `tools/list` no la anuncia para ese
      universo y la llamada falla.
- [ ] Cada escritura de agente queda en el historial con autor agente.

### U7 — Usarlo en varios computadores

**Flujo.** `git pull --rebase` → `universo guard` → trabajar → `git commit` →
`git push`.

- Con historial lineal, los logs y los sellos encajan solos y `diff` cubre
  todo.
- Si dos computadores cambian el criterio a la vez, Git mezcla los bundles.
  `merge=union` conserva las líneas de ambos logs.
  - `guard` marca el estado mezclado como distinto.
  - Se resuelve con `universo check` y `universo seal --motivo "merge"`, que
    con U5 deja la entrada `seal`, y `diff` lo informa por su nombre.
- El conflicto en un sello se resuelve volviendo a sellar.
- Un hook `post-merge` opcional corre `guard` y avisa.

### U8 — Ver cómo crece la base

`universo resumen`, en texto y JSON, para todos los universos:

- notas por tipo y estado, relaciones y reglas;
- candidatas con evidencia suficiente para promover;
- candidatas objetadas;
- notas sin relaciones;
- últimos cambios.

Son conteos sin cuerpos: sirven para decidir qué revisar o promover.

**Aceptación.**
- [ ] Los conteos coinciden con `atomos` en bundles de prueba.
- [ ] «Listas para promover» coincide con lo que la regla de `vigente`
      admitiría.

### U9 — Prueba de punta a punta entre dos clones

Un test con `git` disponible en el CI:

1. `init --git` en A y un remoto «bare» local.
2. En A: capturar tres candidatas, reforzar una en un segundo proyecto y
   promoverla. Commit y push.
3. Clonar en B. `guard` da pass en ambos universos; `search`, `log`, `show` y
   `diff` dan lo mismo que en A.
4. Cambiar en B, push, y pull en A. Sigue encadenado.
5. Cambio concurrente en A y B, y merge. `guard` lo detecta; `check` y `seal`
   lo aceptan; `diff` informa `external_change`.
6. En Windows: clon con `core.autocrlf=true`, y `guard` da pass.

### U10 — Tu primer repositorio de criterio real

Crear con `universo init --git` tu repositorio privado, vacío, como lo hará
cualquier usuario, y usarlo a diario con tus agentes. Toma el lugar de la
prueba de uso real que hacía la instancia legacy. Los hallazgos se reducen a
casos ficticios antes de entrar al repositorio público.

Si conviene traer algo de la biblioteca legacy (D6), se hace a mano, en
privado, sólo con lo que siga vigente. Cuándo pasa a ser tu fuente de
criterio en serio lo decides tú; hasta entonces no reemplaza lo que ya usas.

### U11 — Promover entre universos

**Qué.** `universo promover <nota> --de <escalón> --a <siguiente> --autor <tú>`, para cualquier par de escalones (por ejemplo, de `candidatos` a `vigente`):

1. arma, en el universo de destino, la nota con su texto, tipo, atributos y
   las relaciones `confirmado_en` a sus proyectos (y las notas de proyecto que
   falten);
2. la propone allí: las reglas de `vigente` deciden. Si falla, rechaza con la
   regla y no toca nada;
3. si se admite, marca la candidata como `promovido` en el origen, con
   referencia al destino.

**Límite.** Son dos transacciones, una por universo, no una atómica. El orden
(primero el destino) hace que un fallo intermedio deje, como mucho, una nota
vigente con su candidata sin marcar. `universo resumen` la lista y repetir
`promover` la completa sin duplicar.

**Quién promueve.** En la CLI, Felix no distingue a una persona de un agente.
La protección es por diseño: el MCP no promueve, `AGENTS.md` lo prohíbe, y
las reglas de `vigente` exigen la evidencia. Que la promoción la confirmas tú
es una convención, no algo que Felix pueda garantizar. Lo documentamos así.

**Aceptación.**
- [x] Promover una candidata con evidencia en un solo proyecto se rechaza por
      la regla, sin escribir en ningún universo.
- [x] Con dos proyectos se admite, y la candidata queda `promovido`.
- [x] Un fallo simulado entre las dos transacciones se completa al repetir.
- Hecho en el paso 3 (2026-11-18): `universo promover <nota> --a <universo>
  [--de <universo>]`. El estado `promovido` se agrega al dominio de `estado`,
  y `vigente` suma la regla `solo-criterio-probado` (sólo `vigente` o
  `retirado`). Copia el texto, la validación, la fecha original y las
  relaciones `confirmado_en` (creando los proyectos en el destino); la
  evidencia queda en el origen.
  - `--de` es opcional: por defecto, el universo por defecto del repositorio.
  - Una promoción a medias se reconoce por la marca de procedencia que deja en
    el destino («Promovido desde <origen> (<nota>)»). Otro criterio con el
    mismo título y sin esa marca es `promote_conflict`.
  - `--dry-run` simula los dos pasos.
  - Los atributos extra del frontmatter no se copian al destino.
  - `vigente` no exige `validacion` distinta de `pendiente`: promover es la
    confirmación del dueño.
- `nota editar` aplica sólo cambios en la nota abierta: si el editor toca otra,
  es `edit_outside_note` y no se escribe nada.

### U12 — Reforzar y objetar

**Qué.**

```sh
universo reforzar notas/componentes-reutilizables --proyecto api-pagos \
    --contexto "se aplicó al crear el módulo de cobros" --validacion implicita
universo objetar  notas/componentes-reutilizables --proyecto prototipo-x \
    --contexto "aquí se prefirió duplicar por rapidez de entrega"
```

- Cada uno crea una nota `evidencia` (paráfrasis, proyecto, fecha, tipo de
  validación) con la relación `apoya` u `objeta` hacia el criterio, y la nota
  de proyecto si no existe.
- La evidencia se acumula y no se edita, de modo que el historial muestra
  cómo maduró cada criterio.
- Reforzar dos veces en el mismo proyecto no cuenta doble para la regla de
  `vigente`, porque ésta cuenta proyectos, no evidencias.
- Una objeción en un criterio ya vigente no lo retira sola: `resumen` lo
  marca para que lo revises. Puede que el criterio tenga un ámbito más
  estrecho del que parecía.

**Aceptación.**
- [x] Dos refuerzos en el mismo proyecto dejan un solo `confirmado_en` a ese
      proyecto.
- [x] Objetar deja la evidencia con `objeta` y no confirma.
- [ ] Una objeción aparece en `resumen` (U8) y en `context` del criterio.

### U13 — Detectar el criterio en la conversación

Es la parte difícil: cuándo una frase es criterio y cuándo no.

**Qué.**

- **Guía para el agente.** Va en `AGENTS.md` y en una skill de Claude Code
  instalable desde este repositorio. Define qué señales indican criterio:
  - instrucciones generales sobre cómo hacer algo («quiero que…»,
    «siempre…», «nunca…», «prefiero…»);
  - validaciones explícitas («quedó bien», «así sí»);
  - validaciones implícitas: pasar a otro tema sin objetar, tras una
    instrucción así.

  Define qué no es criterio: órdenes puntuales de la tarea («cambia este
  color»), datos del proyecto, y frases ambiguas. Ante la duda, el agente
  pregunta en una línea antes de capturar.
- **Captura al cerrar.** Un paso opcional al final de una sesión que repasa lo
  conversado, lista las candidatas y te las muestra para aceptar o descartar
  antes de escribir. Convive con los mecanismos que ya usas (destilación de
  conducta, aprendizaje de ECC) sin reemplazarlos.
- **Medición.** Un corpus de conversaciones **ficticias**, con etiquetas
  (criterio o no; validación explícita, implícita o ninguna). Sobre él se
  mide qué tan bien detecta el agente: precisión (no capturar de más) y
  cobertura (no perder criterio). Se corre con Claude. Los resultados se
  publican aunque sean malos, como en la fase 07.

**Aceptación.**
- [ ] La guía y la skill existen y están probadas con el corpus.
- [ ] La precisión medida es alta, por ejemplo ≥ 0,9, porque un falso
      positivo ensucia la base. La cobertura se informa tal cual.
- [ ] Ninguna captura al cierre se escribe sin tu confirmación.

## Hallazgos

- **Simulación y apply no coinciden con nombres repetidos (2026-11-18).**
  Crear dos notas con el mismo nombre en carpetas distintas
  (`criterios/x` y `proyectos/x`) unidas por una arista pasa la simulación,
  pero el apply falla con `snapshot_bundle_invalid`: el enlace `[[x]]` es
  ambiguo al recargar. No se corrompe nada (`guard` sigue en `pass`), pero la
  simulación debería rechazarlo. Los comandos de captura lo evitan con nombres
  únicos; la corrección en el motor queda pendiente.

## Decisiones

Aceptadas el 2026-11-18 tal como las recomienda la última columna. D2 sigue
abierta en su detalle: las plantillas se revisan contigo antes de implementar
U4.


| # | Decisión | Opciones | Recomendación |
|---|---|---|---|
| D1 | Escritura de agentes | (a) sólo CLI; (b) `capture` en el MCP; (c) sólo orquestador | **Resuelta por el diseño de universos, pendiente de tu OK:** los agentes escriben (CLI o MCP) sólo en universos con `escritura_agentes = true` (`candidatos`); `vigente` sólo por promoción |
| D2 | Plantillas (U4) | la propuesta, otra taxonomía, o sólo la mínima | la propuesta, revisada contigo antes de implementarla |
| D3 | `init --git` hace el primer commit | sí / no | no: el primer commit lo haces tú |
| D4 | Felix ejecuta Git más allá de `init` | nunca / comandos de ayuda | nunca |
| D5 | Gates de 09 | `v0.4.0` = evaluación comparativa, o esta fase | esta fase como `v0.4.0` y la evaluación a `v0.5.0` |
| D6 | Traer algo de la biblioteca legacy (U10) | nada / a mano / script | empezar vacío y traer a mano sólo lo vigente |
| D7 | Escalera de universos | dos (`candidatos`, `vigente`) / la escalera completa hasta `axiomas` / por dominio | empezar con dos; `consolidado` y `axiomas` se agregan cuando haya criterio que los merezca, con U5 y U11 (que ya sirven para cualquier par de escalones) |
| D10 | Axiomas como invariantes | sólo prosa / bajar como regla los que se puedan formalizar | bajar como regla los formalizables, siempre con `reglas aplicar` y tu confirmación |
| D8 | Umbral de promoción | dos proyectos / tres / otro criterio | dos proyectos distintos y tu confirmación |
| D9 | Captura al cerrar la sesión (U13) | apagada / sugerir candidatas / capturar sin preguntar | sugerir y esperar tu confirmación |

## Orden propuesto

| Paso | Unidades | Resultado |
|---|---|---|
| 1 | U1, U2 | Un repositorio con universos que se crea con un comando y se usa sin rutas |
| 2 | U3, U12 | Capturar, reforzar y objetar son un comando cada uno |
| 3 | U11, U4 | Promover a `vigente` con reglas que lo exigen |
| 4 | U6, U9 pasos 1–4 | Cualquier agente lo usa; dos clones coinciden |
| 5 | U13 | El agente detecta criterio en la conversación, medido |
| 6 | U5, U7, U9 pasos 5–6 | Reglas que evolucionan y merges entre computadores |
| 7 | U8 | Crecimiento y candidatas listas para promover, visibles |
| 8 | U10 | Tu repositorio de criterio real, creado vacío y en uso diario |

Con los pasos 1 a 4 ya puedes usarlo en local: tus agentes capturan
candidatas, las refuerzan y tú promueves.

## Riesgos

- **Falsos positivos.** El agente puede tomar como criterio algo dicho de
  pasada. Mitigado: todo entra como candidata, `vigente` exige evidencia en
  varios proyectos y tu confirmación, y U13 mide la precisión.
- **Privacidad del contexto.** Se guarda una paráfrasis y la referencia al
  proyecto, no fragmentos de conversación.
- **El acceso de Git es todo o nada.** Lo que no se comparta con las mismas
  personas va en otro repositorio. No guardes secretos ni credenciales.
- **El historial es permanente.** Retirar una nota no la borra de Git.
- **Finales de línea.** Mitigado con `.gitattributes` y la prueba en Windows.
- **Versiones distintas del motor.** Mitigado con `motor` en `felix.toml`.
- **Felix verifica reglas, no la verdad.** Un criterio coherente puede estar
  equivocado. La evidencia, el autor y el motivo permiten revisarlo después.
- **El MCP no protege un clon.** Un agente con acceso al repositorio lo lee
  todo.

## Fuera de alcance

- Servicio alojado, sincronización propia o permisos más finos que Git.
- Cifrado en reposo (por ejemplo, git-crypt): se puede evaluar después.
- Búsqueda semántica, revisión AGM y reparación sugerida.
- Promover sin confirmación humana.

## Verificación prevista

```sh
cargo test -j 2 --workspace --locked
cargo test -j 2 -p universo-cli --test criterio_distribuible   # U9
python tools/check_public_boundary.py
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-11-18 | — | Plan propuesto; regla de promoción probada con `check` sobre `v0.3.0` (un proyecto: reject; dos: admit) | Pendiente de revisión |
