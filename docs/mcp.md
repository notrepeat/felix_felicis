# Loop MCP de propuesta y corrección

`universo-mcp` expone consultas y simulación por `stdio`. El operador fija un
bundle y sus archivos de configuración al iniciar el servidor (o un repositorio
de criterio con `--criterio`). Las herramientas no reciben rutas de archivos del
host ni ofrecen `apply` de deltas arbitrarios, shell, recovery, promoción o una
puerta de publicación. La única escritura son `capture`, `reinforce` y `object`,
sólo con `--criterio` y en universos con `escritura_agentes = true` (ver más
abajo). La lógica de admisión permanece en
`universo-core`; la captura reutiliza la política y cuotas de la simulación OKF.

Con los binarios instalados ([instalación](instalacion.md)):

```sh
universo-mcp --root ./bundle --manifiesto ./manifest.toml --invariantes ./invariants.sx
python tools/demo_mcp.py --binary "$(command -v universo-mcp)" --cli "$(command -v universo)"
```

Con un [repositorio de criterio](plan/10-criterio-distribuible.md) basta
`universo-mcp --criterio <dir> [--universo <nombre>]`: las tres rutas salen de
su `felix.toml` y, sin `--universo`, se usa su universo por defecto. No se
puede mezclar con `--root`, `--manifiesto` ni `--invariantes`. Si `felix.toml`
falta, es inválido, no tiene el universo pedido o pide otra versión del motor,
el servidor no arranca y dice por qué en stderr. `--ignorar-version` salta el
control de versión.

Con `--criterio`, todas las herramientas aceptan un argumento opcional
`universe` (sin él, el universo por defecto o el de `--universo`), y `search`
acepta `universe: "todos"` para buscar en todos, con el universo de cada
resultado. Si algún universo tiene `escritura_agentes = true`, el servidor
también anuncia `capture`, `reinforce` y `object`:

- **`capture`** guarda una afirmación como candidata, con su proyecto, tarea,
  validación y autor (`agente:<author>`). Si ya existe, responde
  `status: "similar"` con las notas existentes.
- **`reinforce`** y **`object`** registran que un criterio se sostuvo, o no,
  en un proyecto.

Escriben con simulación y CAS, y quedan en el historial. Sobre un universo sin
escritura fallan con `universe_read_only`. El MCP nunca promueve.
`universo mcp-config` imprime la configuración del cliente.

En Windows, los ejecutables llevan `.exe`. Un cliente MCP inicia el proceso y
negocia MCP `2025-11-25` mediante `initialize`, envía `notifications/initialized` y usa `tools/list` y
`tools/call`. `stdout` queda reservado al protocolo. El SDK es `rmcp 3.2`, ya
fijado en el workspace; no se introduce runtime async en el núcleo.

## Contrato de herramientas v1

Todos los argumentos llevan `"schema":"felix-mcp-v1"`; se rechazan campos
adicionales. El resultado JSON aparece en `structuredContent` y en el contenido
textual compatible con clientes MCP. Los esquemas se anuncian en `tools/list`.

| Herramienta | Argumentos adicionales | Resultado |
|---|---|---|
| `context` | `node` relativo del grafo; `depth` opcional 0–2; `include_values` opcional, falso por defecto | Subgrafo inducido por vecindad no dirigida de relaciones `Edge` y `Parent`; conteos y procedencia de invariantes. |
| `propose` | `delta` objeto `felix-delta-v1` o `felix-delta-v2` (crea notas) obligatorio | `admit` o `reject`, conteos, clases de conflicto y procedencia de invariantes falsas. Nunca publica. |
| `check` | `delta` opcional | El mismo veredicto sobre el candidato, o sobre la base cuando falta el delta. |
| `why` | `delta` opcional | Lo mismo que `propose`, más `explanations`: la explicación redactada de la primera regla fallida (id, mensaje y forma abstracta del contraejemplo), con el formato de `propose --explain decide --rules`. `explanation_error` es `explanation_not_applicable` si no fallan invariantes, `explanation_budget_exceeded` si la explicación agota su presupuesto, o `explanation_too_large` si su JSON pasa de 1 MB (el rechazo y la procedencia llegan igual). No da rutas, etiquetas ni valores del bundle. |
| `search` | `query` obligatorio (hasta 1024 caracteres, con al menos una palabra); `limit` opcional 1–100, 20 por defecto; `include_values:true` obligatorio | La búsqueda léxica de `universo search` limitada a título, tipo y atributos: notas donde cada término empieza alguna palabra, sin distinguir mayúsculas ni tildes. `summary` (`total`, `returned`) y `matches` con `path`, `type`, `title`, `fields` (unión de los campos donde aparecieron los términos) y `score`. No busca en el cuerpo ni en los campos extra del frontmatter. `search_resource_limit` si la respuesta pasa de 700 KB. |

Ejemplo de llamada:

```json
{"name":"propose","arguments":{"schema":"felix-mcp-v1","delta":{"schema":"felix-delta-v1","add":["Type(n/item, b)"],"remove":[]}}}
```

Un rechazo de dominio es un resultado normal de herramienta (`isError` falso).
Sus `diagnostics` contienen clase y cantidad, y `provenance` identifica sentencia,
línea, columna e instancia cuando procede, más `rule_id` y `rule_message` de la
regla (`null` si no los declara con `(regla :id ... :mensaje ...)`). Las
invariantes de `context` llevan los mismos dos campos. Los escribe el autor de
las reglas, no son datos del bundle; escríbelos sabiendo que los verá el agente.
El MCP no da el testigo localizado (`propose --witness` de la CLI), porque ese
testigo revela rutas. Un error técnico o argumentos
inválidos de una herramienta conocida producen `isError` verdadero con código
redactado. Los errores de envoltura JSON-RPC, método o herramienta desconocida se
distinguen mediante el error de protocolo. Ninguno equivale a un rechazo lógico.

## Contexto y privacidad

`context` sin opt-in revela únicamente conteos y posiciones de fuente. Con
`include_values:true` devuelve átomos canónicos del subgrafo y fórmulas canónicas
de los invariantes. No incluye cuerpos documentales, anexos, comentarios del
programa, rutas absolutas ni tokens de snapshot. Los valores de etiquetas y
atributos presentes en esos átomos sí son información del bundle: solicitar el
opt-in autoriza esa divulgación al cliente conectado.

`search` no tiene modo redactado: aun un conteo de coincidencias confirma que
una palabra está en el bundle, y sondeando prefijos se reconstruiría su
vocabulario. Por eso exige `include_values:true`, y con él revela lo mismo que
`context` con opt-in: rutas, tipos, títulos y valores de atributos. No consulta
el cuerpo ni los campos extra del frontmatter (el anexo), ni siquiera para
contar, así que no sirve para sondearlos. La CLI (`universo search`) sí busca
en ellos y muestra un fragmento del cuerpo, porque es de uso local.

La selección de invariantes es conservadora: devuelve todas las sentencias y
lo declara mediante `invariant_selection:"all_conservative"`. No afirma calcular
la relevancia lógica mínima. La vecindad es un subgrafo inducido: no muestra una
arista cuyo extremo queda fuera de los nodos seleccionados. El orden canónico
permite comparar respuestas entre reinicios.

Cada llamada captura nuevamente los archivos configurados. No hay candidatos
persistidos, memoria de conversación ni identificadores opacos de sesión. Con
los mismos bytes de entrada, reiniciar no cambia el resultado; esto no promete
una instantánea atómica frente a escritores externos entre llamadas.

## Límites y autoridad

| Límite | Valor |
|---|---:|
| Frame de entrada | 1 MiB |
| Profundidad JSON | 32 |
| Respuesta de transporte | 2 MiB |
| Consulta `context` | profundidad 2, 256 nodos, 4096 átomos, 256 invariantes |
| JSON de `context` antes de transporte | 1 MiB |
| Tiempo por llamada al worker | 5 segundos; 5 minutos para `capture`, `reinforce` y `object` |
| Workers activos por servidor | 1 |

El servidor ejecuta el adaptador síncrono en un proceso hijo por llamada. Al
expirar el plazo termina y recoge ese proceso. Las herramientas que escriben
tienen un plazo largo para no cortar un apply a mitad; si aun así vence, la
transacción queda pendiente (`universo status`) y `universo recover` la
completa o la revierte; no abandona una tarea async que
continúe leyendo o calculando. Una llamada concurrente se rechaza como ocupada.
Las cuotas raw de captura OKF y el presupuesto determinista de evaluación del
núcleo siguen vigentes. Estas cotas de bytes no son una promesa de RSS máximo.

El alcance de las herramientas delimita esta API, no es un sandbox del sistema
operativo. Si un cliente/agente recibe además acceso directo al
filesystem o a otra herramienta de escritura, esa autoridad es externa al
servidor. Para el loop de esta fase se concede únicamente esta conexión MCP.
El proceso se ejecuta con permisos del operador y sólo lee sus rutas fijadas;
con `--criterio` y escritura de agentes, también escribe en esos universos,
siempre con simulación, CAS e historial.

**Lo que revelan las herramientas de escritura.** `capture`, `reinforce` y
`object` devuelven las rutas que crearon (nota, proyecto, evidencia), y si el
proyecto ya existía. Una captura duplicada devuelve `status: "similar"` con
las rutas de los criterios existentes. Por eso confirman la existencia de
proyectos y criterios: quien tiene escritura en un universo puede, de hecho,
leer sus nombres.

**Límite de diseño.** Un agente con escritura puede declarar una validación
`explicita` que el dueño no dio, o reforzar en proyectos inventados, y así
reunir la confirmación en dos proyectos que pide `vigente`. Felix no distingue
a una persona de un agente. La salvaguarda es la promoción, que hace el dueño:
`universo promover` muestra las evidencias que respaldan cada confirmación y
quién las registró queda en el historial (`universo log`). Tampoco hay límite
de tasa: un agente que capture sin freno puede agotar el presupuesto de
evaluación del universo.

## Demostración reproducible

`tools/demo_mcp.py` crea una fixture temporal sintética, consulta contexto opt-in,
obtiene el tipo requerido desde la fórmula devuelta y el tipo actual desde los
átomos. Su agente determinista propone el alta incompleta, recibe el conflicto
`Type`, consulta `why`, incorpora el retiro exacto y obtiene admisión. También
compara ambos veredictos y conteos con el ejecutable CLI, reinicia el servidor y
comprueba que todos los archivos y entradas del bundle permanezcan iguales.

```sh
python tools/demo_mcp.py --transcript docs/fixtures/mcp-transcript.json
cargo test -p universo-mcp
cargo test --workspace --test loop_mcp
```

El transcript contiene exclusivamente respuestas de esa fixture sintética.
Este harness demuestra el contrato de integración y corrección; no evalúa un
modelo generativo ni acredita utilidad empírica o desempeño de agentes. Eso
corresponde a la fase 07, fuera del cierre de esta fase.

## Evidencia del servidor directo

Validación local Rust 1.88: workspace 1501 pruebas aprobadas, cero fallos y 19
ignored, en 45 suites; fmt y clippy pasan. Los ignored incluyen helpers de
proceso invocados por pruebas padre y harnesses opt-in. La demo MCP con CLI real
pasó rechazo, corrección, paridad, reinicio e igualdad de archivos;
[transcript sintético](fixtures/mcp-transcript.json). CI del servidor directo `8ac2c69`, ejecución `44146216036`, pasó en Linux
(1501/0/19) y Windows (1477/0/19), con 45 suites por plataforma.

Contrato de referencia: [ciclo de vida MCP 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle)
y [herramientas](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).

## Code Mode opcional

El mismo servidor puede usarse desde el [adaptador Code Mode local](../tools/codemode-local/README.md), con SDK Cloudflare real, workerd y aislamiento adicional de sistema operativo. [La comparación reproducible](codemode-comparacion.md) conserva MCP directo como opción predeterminada; no cambia los permisos ni publica deltas.

La CI Rust/Linux/Windows ejecuta el servidor MCP y sus pruebas; **no ejecuta el
adaptador Code Mode opcional**. Este último tiene validación local en Linux con
bubblewrap: ocho pruebas de aislamiento y 12 muestras A/B, repetidas por revisión
independiente. No se afirma soporte Windows ni cobertura CI para ese adaptador.

Cierre de fase 06: código `6f9eb99`, CI `44147088907` aprobada en ambas
plataformas para el workspace Rust. [Checklist y evidencia completa](plan/06-loop-de-agentes-mcp.md).
