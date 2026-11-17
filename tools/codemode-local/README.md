# Code Mode local opcional

Este adaptador usa **`@cloudflare/codemode 0.5.6` real**, su
`DynamicWorkerExecutor` y un Worker Loader de Miniflare/workerd local. No usa
`eval` ni `node:vm` como sandbox. El motor Rust y los cuatro contratos MCP son
los mismos que en el acceso directo. No se despliega nada a Cloudflare.

Requiere Linux, Node >=22, bubblewrap y user namespaces habilitados. No añade
Node a las dependencias del núcleo ni al servidor MCP; MCP directo conserva su
soporte Linux/Windows. El ejecutor falla si no puede crear el sandbox, sin
fallback de menor aislamiento.

```sh
# Desde este directorio; instalar no ejecuta lifecycle scripts.
npm run setup
npm test
node compare.mjs ../../target/debug/universo-mcp 3
```

El bootstrap copia el manifest y lockfile a `target/codemode-local`, instala
allí con `npm ci --ignore-scripts` y genera el bundle ignorado. No crea
`node_modules` entre las fuentes. `FELIX_CODEMODE_CACHE` permite elegir otra
ruta de caché; usa la misma variable al preparar y ejecutar.

Para un programa elegido por el operador:

```sh
node run.mjs /ruta/universo-mcp /ruta/bundle /ruta/manifest.toml /ruta/invariants.sx /ruta/programa.js
```

El archivo del programa debe ser regular, UTF-8 y como máximo 64 KiB. Por ejemplo:

```js
async () => {
  const state = await felix.check({schema: "felix-mcp-v1"});
  return {status: state.status, summary: state.summary};
}
```

El único namespace de capacidades es `felix`, con `context`, `propose`, `check`
y `why`; sus argumentos son los de [MCP](../../docs/mcp.md). No se expone `apply`.
El operador configura rutas fuera del programa. El cliente Node host conserva
la conexión MCP stdio y ejecuta los callbacks autorizados. El sandbox no recibe
rutas, handles ni bytes del bundle excepto los resultados solicitados por esas
cuatro herramientas. `context` necesita el mismo opt-in de valores en ambos modos.

## Aislamiento y límites

La guía oficial advierte que Miniflare por sí solo no es un sandbox endurecido.
Por eso el proceso entero Node/Miniflare/workerd se ejecuta dentro de bubblewrap:
namespace de red separado, namespace PID, entorno vacío, `/usr`, librerías,
binario Node y directorio del adaptador montados sólo lectura, `/tmp` efímero y
sin montaje del bundle, home ni credenciales. Los únicos mensajes hacia el host
son callbacks JSON por pipes heredados y un resultado final. El programa no
obtiene acceso directo a esos pipes ni a los bindings internos del host Node.

`DynamicWorkerExecutor` configura además `globalOutbound:null`: `fetch` y
conexiones salientes del Worker quedan bloqueados. El entorno `process.env`
emulado puede existir, pero no contiene las variables del host. Se valida la
lista de cuatro métodos y el tamaño de argumentos en el host, antes de llamar
MCP; las validaciones del servidor siguen vigentes.

| Recurso | Cota |
|---|---:|
| Código UTF-8 | 64 KiB |
| Argumentos por callback | 64 KiB |
| Callbacks por ejecución | 16 |
| Callbacks simultáneos | 1 |
| stdout+stderr del sandbox, incluidos logs/resultado | 1 MiB |
| Respuesta individual enviada al sandbox | 1 MiB |
| Plazo SDK interno | 10 s |
| Plazo externo de ejecución, arranque incluido | 12 s |

El supervisor externo termina el grupo de procesos, incluidos descendientes,
y espera el cierre incluso ante un bucle síncrono infinito. El cliente MCP
propio también termina su grupo al cerrar. No se afirma una cota de memoria RSS,
ni inmunidad ante errores del kernel, bubblewrap, V8 o workerd. Instalar paquetes
es una operación del operador previa; el programa no dispone de npm ni de red
externa. Esta integración sólo reclama el aislamiento comprobado por sus pruebas.

## Comparación reproducible

`compare.mjs` usa el mismo agente determinista y fixture en ambos modos. Deriva
el tipo requerido desde el invariante devuelto y el retiro exacto desde el
contexto: consulta, propuesta incompleta, explicación y propuesta corregida.
MCP directo hace cuatro invocaciones exteriores; Code Mode ejecuta ese mismo
programa en una sola invocación exterior, conservando **cuatro llamadas MCP
internas**. Se verifica igualdad de resultados y que el bundle no cambie.

Las muestras frías incluyen creación del cliente MCP, initialize, tools/list y
la secuencia. Las cálidas reutilizan ese cliente MCP; **cada ejecución Code Mode
crea un sandbox nuevo**, incluso en las muestras cálidas. Los bytes de frontera
son JSON UTF-8 de los argumentos/resultados: en directo son cuatro pares; en
Code Mode incluyen el código completo y su resultado. El catálogo se mide aparte,
no se descuenta del contexto ni se afirma que el wrapper lo elimine.

No interviene un modelo, tokenizer ni proveedor de inferencia. Los números no
miden ahorro de tokens, dinero, calidad de razonamiento ni latencia de red de un
LLM. Véanse [muestras y decisión](../../docs/codemode-comparacion.md).

Fuentes primarias: [SDK y API](https://developers.cloudflare.com/agents/tools/codemode/api-reference/),
[ejecución local y advertencia de aislamiento](https://developers.cloudflare.com/dynamic-workers/examples/dynamic-workers-starter/),
[Code Mode sobre MCP](https://developers.cloudflare.com/agents/model-context-protocol/guides/build-codemode-mcp-server/).
