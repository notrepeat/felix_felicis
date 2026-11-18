# Loop MCP de propuesta y corrección

`universo-mcp` expone consultas y simulación por `stdio`. El operador fija un
bundle y sus archivos de configuración al iniciar el servidor. Las herramientas
no reciben rutas de archivos del host ni ofrecen `apply`, escritura, shell,
recovery o una puerta de publicación. La lógica de admisión permanece en
`universo-core`; la captura reutiliza la política y cuotas de la simulación OKF.

```sh
cargo build -p universo-mcp -p universo-cli
./target/debug/universo-mcp --root ./bundle --manifiesto ./manifest.toml --invariantes ./invariants.sx
python tools/demo_mcp.py --binary target/debug/universo-mcp --cli target/debug/universo
```

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
| `why` | `delta` opcional | Clases funcionales y procedencia disponibles para ese candidato/base; no inventa un testigo ni repite valores privados. |

Ejemplo de llamada:

```json
{"name":"propose","arguments":{"schema":"felix-mcp-v1","delta":{"schema":"felix-delta-v1","add":["Type(n/item, b)"],"remove":[]}}}
```

Un rechazo de dominio es un resultado normal de herramienta (`isError` falso).
Sus `diagnostics` contienen clase y cantidad, y `provenance` identifica sentencia,
línea, columna e instancia cuando procede. Un error técnico o argumentos
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
| Tiempo por llamada al worker | 5 segundos |
| Workers activos por servidor | 1 |

El servidor ejecuta el adaptador síncrono en un proceso hijo por llamada. Al
expirar el plazo termina y recoge ese proceso; no abandona una tarea async que
continúe leyendo o calculando. Una llamada concurrente se rechaza como ocupada.
Las cuotas raw de captura OKF y el presupuesto determinista de evaluación del
núcleo siguen vigentes. Estas cotas de bytes no son una promesa de RSS máximo.

La ausencia de herramientas de escritura delimita esta API, no es un sandbox
del sistema operativo. Si un cliente/agente recibe además acceso directo al
filesystem o a otra herramienta de escritura, esa autoridad es externa al
servidor. Para el loop de esta fase se concede únicamente esta conexión MCP.
El proceso se ejecuta con permisos del operador y sólo lee sus rutas fijadas.

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
