---
name: felix-criterio
description: Detecta el criterio que el dueño expresa en la conversación (instrucciones generales, validaciones explícitas o implícitas) y lo guarda como candidato en su repositorio de criterio de Felix. Úsala al trabajar en un proyecto con un repositorio de criterio (felix.toml) o con el MCP de Felix conectado, y al cerrar una sesión.
---

# Detectar y guardar criterio

El dueño guarda su criterio (decisiones, principios, preferencias) en un
repositorio de Felix. Tu trabajo es notar cuándo lo expresa y guardarlo como
**candidato**, sin llenar la base de ruido: un falso positivo ensucia la base
más de lo que ayuda una captura de más.

## Qué es criterio

Una frase del dueño es criterio si vale **más allá de la tarea de hoy**: dice
cómo quiere que se hagan las cosas en general.

- **Instrucción general:** «quiero que…», «siempre…», «nunca…»,
  «prefiero…», «en mis proyectos…», «la regla es…». Categoría `explicita`.
- **Validación explícita de un enfoque general:** tú propusiste o aplicaste
  una forma general de trabajar y el dueño la aprueba con palabras («así sí»,
  «eso es lo que quiero siempre», «quedó bien, hazlo así de ahora en
  adelante»). Categoría `explicita`.
- **Validación implícita:** tú anunciaste que seguirás una forma general de
  trabajar («de ahora en adelante separo los tests por módulo») y el dueño
  sigue con otro tema sin objetar. Categoría `implicita`. Sólo cuenta si lo
  que anunciaste es general y el dueño tuvo ocasión de leerlo.

Una forma general de trabajar cuenta aunque se diga dentro de un proyecto
(«documento cada función pública con un ejemplo»); lo que no cuenta son los
hechos que sólo describen ese proyecto.

## Qué no es criterio

Categoría `ninguna`:

- **Órdenes puntuales de la tarea:** «sube el tamaño del título», «corrige
  la errata del README», «mueve este archivo a utils».
- **Datos del proyecto:** nombres, fechas, versiones, credenciales,
  decisiones que sólo describen este proyecto («el servidor de pruebas está en Fráncfort»).
- **Excepciones y alcance local:** «sólo esta vez…», «sólo aquí…», «de
  momento…», «en esta función…».
- **«Siempre» o «nunca» sin instrucción:** «siempre pasa lo mismo con este
  servidor», «nunca me había pasado».
- **Hipótesis, preguntas y lluvia de ideas:** «¿convendría…?», «tal vez…».
- **Lo que otro dijo:** «el cliente pidió…» sin que el dueño lo
  haga suyo.
- **Aprobaciones de un resultado concreto** que no dicen nada general
  («genial», «está bien así»).
- **Frases ambiguas.** Si no sabes si es general, no la clasifiques como
  criterio: pregunta.

## Ante la duda, pregunta

Si una frase podría ser criterio pero no estás seguro de que sea general,
pregunta en una línea antes de capturar: «¿Lo guardo como criterio general
(…paráfrasis…) o es sólo para esta tarea?». No captures sin respuesta.

## Cómo guardar

Guarda una **paráfrasis concisa** en tercera persona impersonal, no la
conversación. Con la CLI, dentro del repositorio de criterio:

```sh
universo capturar "<paráfrasis>" --proyecto <proyecto> --tarea <tarea> \
  --validacion explicita|implicita --autor agente:<nombre>
```

Con el MCP: `capture` con los mismos datos. Si responde `capture_similar`, ya
existe: **refuerza** esa nota (`universo reforzar <nota> --proyecto <p>
--contexto <c>` o `reinforce`) en vez de duplicarla. Si el dueño contradice un
criterio guardado: `universo objetar` (`object`).

Antes de decidir algo sobre lo que el dueño podría tener opinión, consulta:
`universo search <términos> --universo vigente`, luego `candidatos` (MCP:
`search` con `universe: "todos"`).

## Captura al cerrar

Al final de una sesión, si el dueño lo pide o lo acepta:

1. Repasa la conversación con las reglas de arriba.
2. Lista las candidatas: paráfrasis, categoría y la frase que la origina.
3. **Espera su confirmación de cada una.** Guarda sólo las que acepte, con los
   cambios que pida. Ninguna captura al cierre se escribe sin confirmación.

Esto convive con otros mecanismos de memoria o aprendizaje que el dueño use;
no los reemplaza.

## Lo que no haces

- No promueves (`universo promover`): lo decide el dueño.
- No escribes fuera de `candidatos`, no editas a mano los archivos de
  `universos/` y no ejecutas `universo seal`.
- No guardas secretos, credenciales ni datos personales de terceros.
