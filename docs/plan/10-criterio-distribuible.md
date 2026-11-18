# 10 — Criterio distribuible: la herramienta aquí, el criterio en su propio repositorio

El objetivo es que Felix sirva en local, para ti y para cualquier agente, como
**fuente de criterio**: no un almacén de datos, sino de decisiones, principios
y reglas que se verifican antes de aceptarse. El criterio vive en un
repositorio Git propio y privado, separado de este. Así se puede llevar a otro
computador y compartir sólo con quien se invite.

Esta es la fase 10 del [plan maestro](00-plan-maestro.md). No es el «paso 10»
de las [conclusiones](../conclusiones-ia-felix-felicis.md) (reparación
sugerida, revisión AGM, recuperación avanzada), que sigue fuera de alcance.

## Estado

| Campo | Valor |
|---|---|
| Estado | propuesto (2026-11-18), pendiente de revisión |
| Depende de | `v0.3.0` (búsqueda, historial con efecto, `diff`, instalación) |
| Desbloquea | Uso diario real; migración opcional de la biblioteca privada |

## Punto de partida: toda base empieza vacía

Quien instale Felix, tú incluido, empieza con una base sin criterio. Eso fija
las prioridades:

1. **Al principio, el trabajo es capturar.** Las primeras semanas casi todo el
   uso será guardar criterio: «decidí X por Y», «prefiero A sobre B salvo
   que…», «esto ya no vale, lo reemplaza aquello». Si capturar cuesta, la base
   no crece y la herramienta nunca llega a ser útil.
2. **Las reglas llegan después.** Con pocas notas no hay mucho que verificar.
   Las invariantes aparecen a medida que el criterio se repite y se ve qué
   debe cumplirse siempre. Cambiar reglas tiene que ser un paso normal y
   registrado, no una edición a mano que rompe el historial.
3. **El valor crece con el uso.** Consultar sirve cuando ya hay qué
   encontrar. Hay que hacer visible ese crecimiento (cuántas notas, cuántas
   reglas, qué quedó en borrador) para saber cuándo la base empieza a pagar.

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
- Todos los comandos piden `--manifiesto` y `--invariantes` con su ruta. El MCP
  arranca con `--root`, `--manifiesto` y `--invariantes`.
- Crear una nota hoy exige escribir un delta `felix-delta-v2` en JSON, o editar
  una copia del bundle y usar `universo delta --desde`. Funciona, pero cuesta
  demasiado para capturar a diario.
- El MCP consulta, simula y explica, pero no escribe. Publicar pasa por la CLI
  (`propose --apply`) o por el orquestador.
- `seal` acepta el estado actual sin dejar entrada en el historial. Por eso,
  editar las reglas a mano y sellar deja un hueco que `diff` informa como
  `history_gap`.
- Hay un hook de pre-commit (`examples/guardia/pre-commit`) que corre `guard`.

## Arquitectura

```text
felix_felicis (público, este repo)       mi-criterio (privado, tuyo)
───────────────────────────────────      ─────────────────────────────
motor, CLI `universo`, MCP               felix.toml        ← qué es y qué motor espera
docs, ejemplos ficticios                 AGENTS.md         ← cómo lo usa un agente
releases con binarios                    README.md
                                         manifest.toml     ← tipos, relaciones, atributos
          instala ──────────────►        invariants.sx     ← reglas del criterio
          `universo` y `universo-mcp`    bundle/…          ← las notas
                                         .bundle.felix.log ← historial
                                         .bundle.felix.seal
                                         .gitignore  .gitattributes
```

- El motor no conoce dónde está ningún repositorio de criterio, como hoy con
  la instancia privada (fase 08).
- Un repositorio de criterio declara la versión de motor que espera. Así dos
  computadores con versiones distintas de `universo` no lo interpretan
  distinto sin avisar.
- Git da copia, respaldo, historial y control de acceso. Felix no hace `push`
  ni `pull`: eso queda en manos del usuario o del agente que opera Git.

## Unidades de trabajo

Cada unidad se hace con TDD, revisión independiente y CI verde, como las fases
anteriores.

### U1 — `felix.toml`: un repositorio que se describe solo

**Qué.** Un archivo en la raíz del repositorio de criterio:

```toml
schema = "felix-criterio-v1"
motor = ">=0.4, <0.5"          # versiones de universo compatibles
bundle = "bundle"
manifiesto = "manifest.toml"
invariantes = "invariants.sx"
```

**Descubrimiento.** Todos los comandos resuelven el repositorio en este orden:

1. las opciones explícitas actuales (`--manifiesto`, `--invariantes` y la
   raíz), que siguen funcionando igual;
2. `--criterio <dir>`;
3. la variable `FELIX_CRITERIO`;
4. el `felix.toml` más cercano, subiendo desde el directorio actual.

Así basta `universo check` o `universo search cache` dentro del repositorio, o
desde cualquier lugar con `FELIX_CRITERIO` definida.

**Versión.** Si la versión de `universo` no cumple `motor`, el comando falla
con `engine_version_mismatch` y dice cuál pide. `--ignorar-version` lo
salta, de forma explícita.

**Aceptación.**
- [ ] `universo check`, `search`, `log`, `show`, `diff`, `guard`, `propose` y
      `status` funcionan sin rutas dentro de un repositorio con `felix.toml`.
- [ ] La precedencia está probada caso por caso.
- [ ] Las rutas de `felix.toml` son relativas a él y no pueden salir del
      repositorio (`..`, absolutas): error `criterio_path_invalid`.
- [ ] El MCP acepta `--criterio <dir>` como alternativa a sus tres rutas.

### U2 — `universo init --git`: un repositorio de criterio listo

**Qué.** `universo init <dir> --git` hace lo de hoy y además:

- escribe `felix.toml`, `README.md` y `AGENTS.md` (ver U6);
- escribe un `.gitignore` con los transitorios listados arriba. Nunca ignora
  el historial ni el sello.
- escribe un `.gitattributes` con `* text eol=lf`. Sin esto, Git en Windows
  puede convertir a CRLF. Comprobado sobre `v0.3.0`: con una nota en CRLF,
  `validate` sigue aceptando, pero `guard` da `fail` porque el sello ya no
  coincide, y un cuerpo con `\r` que entra por un delta se rechaza
  (`note_invalid_body`). Además declara `merge=union` para
  `.bundle.felix.log` (ver U7).
- hace `git init` y deja el hook de pre-commit de `guard` instalado, pero
  apagado. Se activa con una línea que el README indica.

No hace el primer commit ni configura un remoto. Lo propongo así; es una
decisión tuya (ver abajo).

**Aceptación.**
- [ ] `git status` tras `init --git` y un apply muestra sólo archivos que deben
      versionarse.
- [ ] Un clon en otro directorio pasa `guard` sin cambios.
- [ ] Un clon en Windows con `core.autocrlf=true` también pasa `guard`.
      Pruebas en el job de Windows del CI.

### U3 — Capturar sin fricción

Es lo más importante para una base vacía. Son comandos que arman el delta,
lo simulan y lo aplican en un solo paso, con CAS interno, autor y motivo:

```sh
universo nota nueva "Prefiero SQLite para prototipos" --tipo criterio \
    --cuerpo-desde nota.md --autor yo --motivo "decisión del proyecto X"
universo nota editar notas/prefiero-sqlite      # abre $EDITOR y aplica el cambio
universo relacion notas/benchmark apoya notas/prefiero-sqlite
universo atributo notas/prefiero-sqlite estado vigente
```

- `nota nueva` crea la ruta a partir del título (minúsculas, sin tildes,
  guiones) y la muestra, o la toma de `--ruta`.
- `nota editar` copia la nota a un borrador temporal, abre el editor, y al
  cerrar usa lo que ya hace `universo delta --desde` y aplica. Si el editor
  se cierra sin cambios, no hay apply.
- `relacion` y `atributo` construyen deltas `felix-delta-v1`. `atributo`
  reemplaza el valor anterior si el atributo es de un solo valor.
- Todos aceptan `--dry-run` (simular y mostrar el veredicto sin escribir) y
  `--format json`.
- Un rechazo muestra la regla que falla (id y mensaje) y, en terminal, el
  testigo localizado, que es de uso local.

**Aceptación.**
- [ ] Una nota se crea con un comando, sin escribir JSON.
- [ ] Cada comando deja una entrada de historial con su efecto.
- [ ] Un rechazo no modifica nada (mismo criterio que `propose`).
- [ ] La ruta generada es estable para el mismo título, y un choque con una
      existente es error, no sobrescritura.

### U4 — Una plantilla de criterio útil desde el primer día

La plantilla de `init` es mínima. Propongo una opcional,
`universo init --plantilla criterio`, pensada para guardar criterio (es una
decisión tuya):

| Elemento | Propuesta |
|---|---|
| Tipos | `principio`, `decision`, `preferencia`, `antipatron`, `fuente` |
| Relaciones | `apoya` (fuente o nota → nota), `reemplaza` (nota nueva → anterior), `contradice` (para marcar tensiones a resolver), `aplica_a` (nota → ámbito) |
| Atributos | `estado` (`borrador`, `vigente`, `retirado`), `confianza` (`baja` < `media` < `alta`, ordenado), `vigente_desde` (fecha), `ambito` (varios valores) |
| Reglas iniciales | sólo un criterio no `fuente` puede estar `vigente`; lo que se `reemplaza` queda `retirado`; una nota `retirado` tiene quien la reemplace; no hay `contradice` entre dos notas `vigente` |

La plantilla se puede cambiar después con U5. No se impone ninguna taxonomía:
la mínima actual sigue siendo la de por defecto.

**Aceptación.**
- [ ] `init --plantilla criterio` pasa `check` y tiene una nota de ejemplo de
      cada tipo, en `borrador`.
- [ ] Cada regla inicial tiene un test que la hace fallar y otro que la
      cumple.

### U5 — Cambiar reglas y manifiesto sin romper el historial

**Qué.** `universo reglas aplicar <nuevo.sx>` y `universo manifiesto aplicar
<nuevo.toml>`:

1. toman el lock y comprueban el estado admitido (CAS);
2. verifican el bundle actual con la configuración nueva. Si alguna nota
   incumple una regla nueva, rechazan y dicen cuál;
3. instalan el archivo nuevo;
4. registran una entrada de historial de clase `config` con autor, motivo,
   tokens `from` y `to`, y el efecto: el manifiesto puede cambiar qué átomos
   salen de las mismas notas.

Además, `seal` pasa a registrar una entrada de clase `seal` (cambio externo
aceptado), con `from`, `to` y sin efecto.

**Resultado.** La cadena de estados no se rompe. `diff` a través de una
entrada `config` muestra «reglas cambiadas en la entrada n». A través de una
`seal` responde `external_change` con la entrada, en vez de un
`history_gap` sin explicación.

**Aceptación.**
- [ ] Endurecer una regla que el bundle ya incumple se rechaza sin escribir.
- [ ] Tras `reglas aplicar`, `diff` sobre el rango funciona y lo marca.
- [ ] `seal` de una edición externa deja la entrada `seal`, y `diff` la
      informa por su nombre.
- [ ] Los lectores de `felix-log-v1` anteriores siguen leyendo el log, porque
      la clase es un campo opcional.

### U6 — Que cualquier agente lo use

**Qué.**

- **`AGENTS.md` en el repositorio de criterio**, generado por `init`.
  Explica cuándo consultar (antes de decidir, con `search` y `context`), cómo
  proponer y qué no hacer: no editar archivos del bundle a mano, no sellar.
- **`universo mcp-config --cliente claude-code|codex|json`** imprime la
  configuración lista para pegar, con rutas absolutas al clon local. Para
  Claude Code recomienda registrarlo a nivel de usuario (`claude mcp add
  --scope user ...`), porque el criterio se consulta desde cualquier
  proyecto, no sólo desde el suyo.
- **Escritura de agentes.** Hoy el MCP no escribe, por decisión de diseño
  (fase 06). Opciones:
  - (a) El agente usa la CLI de U3, si tiene shell. No cambia nada del MCP.
  - (b) Una herramienta `capture` en el MCP, desactivada por defecto. Sólo
    existe si el servidor arranca con `--permitir-escritura`, y aplica con
    CAS, autor `agente:<nombre>` y motivo obligatorio.
  - (c) Sólo mediante el orquestador.

  Recomiendo (a) más (b) apagada por defecto. Es una decisión tuya, porque
  revierte una regla de la fase 06.

**Aceptación.**
- [ ] Con sólo `AGENTS.md` y la configuración MCP, un agente de prueba
      encuentra una nota con `search`, propone una nueva, recibe un rechazo
      explicado y lo corrige.
- [ ] Con (b) apagada, `tools/list` no anuncia `capture`.
- [ ] Con (b) encendida, cada escritura queda en el historial con autor
      agente.

### U7 — Usarlo en varios computadores

**Flujo recomendado.** `git pull --rebase` → `universo guard` → capturar o
proponer → `git commit` → `git push`.

- Con historial lineal, el log y el sello encajan solos y `diff` cubre todo.
- Si dos computadores cambian el criterio a la vez, Git mezcla el bundle.
  Con `merge=union`, el log conserva las líneas de ambos.
  - El estado mezclado no lo produjo ningún apply: `guard` lo marca
    distinto y la cadena tiene un salto.
  - Se resuelve con `universo check` y luego `universo seal --motivo "merge"`,
    que con U5 deja la entrada `seal`.
  - `diff` informa el salto por su nombre.
- El conflicto en `.bundle.felix.seal` se resuelve volviendo a sellar. No
  tiene contenido que conservar.
- Un hook `post-merge` opcional corre `guard` y avisa.

**Aceptación.** Ver U9.

### U8 — Ver cómo crece la base

**Qué.** `universo resumen`, en texto y JSON:

- notas por tipo y por estado, relaciones y reglas;
- notas sin ninguna relación;
- borradores con más de N días;
- últimos cambios (de `log`).

Son sólo conteos, sin juicios. Sirve para decidir qué capturar o qué regla
escribir.

**Aceptación.**
- [ ] Los conteos coinciden con `atomos` en un bundle de prueba.
- [ ] La salida JSON no incluye cuerpos.

### U9 — Prueba de punta a punta entre dos clones

**Qué.** Un test, en Rust o Python, con `git` disponible en el CI:

1. `init --git` en un directorio A y un remoto «bare» local.
2. Capturar tres notas, una relación y un atributo en A. Commit y push.
3. Clonar en B. `guard` da pass. `search`, `log`, `show` y `diff` dan lo
   mismo que en A.
4. Cambiar en B, hacer push, y pull en A. Sigue encadenado.
5. Cambio concurrente en A y en B, y merge. `guard` lo detecta; `check` y
   `seal` lo aceptan; `diff` informa `external_change` en esa entrada.
6. En Windows: clon con `core.autocrlf=true`, y `guard` da pass.

### U10 — Migrar tu biblioteca privada (opcional)

Convertir la biblioteca de la fase 08 en un repositorio de criterio, sólo en
el repositorio privado, con su propio `felix.toml`. Es la primera prueba
real del flujo.

Cuándo hacerlo, y si pasa a ser tu fuente de criterio en serio, lo decides
tú. Hasta entonces, Felix sigue en construcción y no reemplaza lo que ya
usas.

## Decisiones que necesito de ti

| # | Decisión | Opciones | Recomendación |
|---|---|---|---|
| D1 | Escritura de agentes por MCP (U6) | (a) sólo CLI; (b) `capture` apagada por defecto; (c) sólo orquestador | (a) + (b) apagada |
| D2 | Plantilla de criterio (U4) | la propuesta, otra taxonomía, o sólo la mínima | la propuesta, como opción |
| D3 | `init --git` hace el primer commit | sí / no | no: el primer commit lo haces tú |
| D4 | Felix ejecuta Git más allá de `init` | nunca / comandos de ayuda (`universo sync`) | nunca: Git queda en tus manos |
| D5 | Gates de 09 | mantener `v0.4.0` = evaluación comparativa, o pasar esta fase a `v0.4.0` | esta fase como `v0.4.0` y la evaluación a `v0.5.0` |
| D6 | Migrar la biblioteca privada (U10) | ahora, después de U9, o no | después de U9 |

## Orden propuesto

| Paso | Unidades | Resultado |
|---|---|---|
| 1 | U1, U2 | Un repositorio de criterio que se crea con un comando y se usa sin rutas |
| 2 | U3 | Capturar criterio es un comando por cosa |
| 3 | U6 (sin `capture`), U9 pasos 1–4 | Un agente lo usa; dos clones coinciden |
| 4 | U5, U7, U9 pasos 5–6 | Reglas que evolucionan y merges entre computadores, sin perder el historial |
| 5 | U4, U8 | Plantilla útil y crecimiento visible |
| 6 | U10 | Tu biblioteca real como repositorio de criterio, si lo decides |

Los pasos 1 a 3 bastan para empezar a usarlo en local con una base vacía.

## Riesgos

- **Finales de línea.** Mitigado con `.gitattributes` y con la prueba en
  Windows de U9.
- **El acceso de Git es todo o nada.** Quien entra al repositorio ve todo el
  criterio y su historial, incluido lo borrado. Lo que no se pueda compartir
  va en otro repositorio. No guardes secretos ni credenciales en el criterio.
- **El historial es permanente.** Retirar una nota no la borra de Git.
- **Versiones distintas del motor.** Mitigado con `motor` en `felix.toml`.
- **Felix verifica reglas, no la verdad.** Un agente puede capturar criterio
  coherente pero equivocado. El autor y el motivo en cada entrada permiten
  revisarlo después.
- **El MCP no protege un clon.** Un agente con acceso al repositorio lee todo,
  sin pasar por el filtro de `include_values`.

## Fuera de alcance

- Servicio alojado, servidor de sincronización o permisos más finos que los
  del repositorio Git.
- Cifrado en reposo (por ejemplo, git-crypt). Se puede evaluar después, pero
  no lo implementa Felix.
- Búsqueda semántica, revisión AGM y reparación sugerida.

## Verificación prevista

```sh
cargo test -j 2 --workspace --locked
cargo test -j 2 -p universo-cli --test criterio_distribuible   # U9
python tools/check_public_boundary.py
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-11-18 | — | Plan propuesto | Pendiente de revisión |
