# MCP directo y Code Mode local: comparación de fase 06

Se mantiene **MCP directo como interfaz predeterminada** y Code Mode como
adaptador local opcional. La composición permite reducir cuatro invocaciones
exteriores a una en el escenario sintético, pero añade aproximadamente 400 ms
de arranque/ejecución local en estas muestras. No hay evidencia de mejora de
coste o calidad de un modelo: no se utilizó ninguno.

La implementación opcional usa el SDK Cloudflare real con workerd local y una
frontera adicional bubblewrap. Los bundles permanecen locales; no requiere
cuenta, credenciales, despliegue ni conexión a Cloudflare al ejecutar. Su
plataforma actual es Linux; el servidor MCP Rust funciona también en Windows.
Instrucciones y límites en [el adaptador](../tools/codemode-local/README.md).

## Medición

Comando desde `tools/codemode-local`, con el binario debug local de MCP:

```sh
node compare.mjs ../../target/debug/universo-mcp 3
```

Registro íntegro: [codemode-comparison.json](fixtures/codemode-comparison.json).
Node `v24.21.0`; SDK `0.5.6`; Miniflare `4.20260730.0`. Tres repeticiones de cada
modo, cada una con muestra fría y cálida, sobre la misma fixture Type sintética.
Las cifras de tiempo siguientes son medianas en milisegundos, no un SLA.

| Medida | MCP directo | Code Mode local |
|---|---:|---:|
| Frío, incluyendo inicio MCP y catálogo | 14.722 | 416.736 |
| Cálido, cliente MCP reutilizado | 11.471 | 423.098 |
| Invocaciones exteriores por secuencia | 4 | 1 |
| Llamadas MCP internas por secuencia | 4 | 4 |
| Bytes JSON en frontera del agente, sin catálogo | 1354 | 1130 |
| Bytes JSON del catálogo upstream, separado | 1792 | 1792 |

Code Mode siempre crea un nuevo sandbox por ejecución; «cálido» sólo significa
que reutiliza el cliente MCP. La frontera del agente directo cuenta argumentos y
resultados de las cuatro herramientas; la de Code Mode cuenta el código completo
más el resultado final seleccionado. Los bytes de transporte MCP por solicitud y
respuesta también están en el registro; las diferencias de un byte entre llamadas
frías/cálidas provienen de los identificadores JSON-RPC. Son bytes UTF-8 medidos,
no una estimación de tokens.

## Resultado y alcance

Ambos modos consultan contexto con el mismo opt-in, derivan del resultado el tipo
requerido y el tipo actual, obtienen un rechazo `Type`, consultan `why` y readmiten
la corrección. El resultado final es igual y no se escribe el bundle. El programa
no recibe un delta corregido preconstruido por fuera de las herramientas.

El acceso directo es más simple y rápido en este catálogo de cuatro herramientas
locales. Code Mode ofrece composición y filtrado programáticos útiles cuando se
quiere conservar resultados intermedios fuera de la conversación. No se extrapola
el beneficio a catálogos masivos ni a latencia real de inferencia. La fase 07 podrá
medir modelos, utilidad y coste; esta comparación no inicia ni completa esa fase.

## Evidencia local

La instalación outtree mediante `npm run setup` pasó con el lockfile y scripts
de dependencias deshabilitados. La suite `npm test` pasó 8/8: SDK/callback real,
red y entorno, capacidad ausente y cuota de llamadas, bucle infinito con
terminación de descendientes workerd, límites de resultado/logs, canario de
archivo host ausente incluso desde el runtime Node confiable, cierre MCP
idempotente tras señal y Unicode multibyte atravesando los pipes. Una revisión
independiente repitió las ocho pruebas con éxito.

El benchmark incluye 12 muestras en total y todos los resultados coinciden.
Las cifras son de esta máquina y ejecutable debug; repetirlas puede cambiar
el tiempo. La comparación no modifica los contratos ni las cuotas del motor.
