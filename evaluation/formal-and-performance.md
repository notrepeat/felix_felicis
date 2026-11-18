# Corpus formal y costo operativo

## Corpus etiquetado

`formal_corpus.tsv` declara 53 casos sobre una fixture fija de tres nodos,
dos aristas paralelas, una cadena Parent y un atributo Nivel (ordenado
`bajo < alto` desde 2026-11-18, para las comparaciones). Las etiquetas
fueron escritas a partir de esos hechos, no obtenidas llamando al evaluador.
`crates/universo-core/tests/support/formal_fixture.rs` define los hechos exactos.

El test `formal_corpus.rs` compara cada resultado con su etiqueta: 28 admit,
23 rechazos de invariantes y dos errores técnicos por constantes inexistentes.
Un `match` exhaustivo recorre el AST y exige las 16 variantes Formula: Type,
Edge, Parent, HasLab, Ext, Compara (comparación de orden), TcParent, ambas
igualdades, No, Y, O, Implica, ParaTodo, Existe y AlMenos (cardinalidad `al-menos`). Incluye variables nodo/arista, constantes, guardas, clausura
estricta, azúcar Raiz y esquemas Lab/KV/KE/Dom.Nivel. Un test adicional verifica
rechazo de las tres clases de conflicto funcional Type, HasLab y Parent.

El resultado registrado contiene cero falsos admit y coincidencia exacta de
las etiquetas. Esto cubre los constructores y clases indicados; no demuestra
corrección para toda fórmula posible ni sustituye las pruebas de propiedades,
oráculo de referencia, presupuestos y crash recovery del workspace. Los errores
técnicos se mantienen separados de los rechazos lógicos.

```sh
python evaluation/run_formal.py --output evaluation/formal-results.json
```

Desde un clon limpio primero se descargan las dependencias fijadas con
`cargo fetch --locked`; las recetas usan después `--locked --offline`. La
Toolchain 1.88.0 está fijada en el repositorio. El registro incluye hashes del
corpus, fixture y test para identificar exactamente lo medido.

## Matriz de costo del motor

```sh
python evaluation/benchmark_matrix.py --repetitions 5 --output evaluation/benchmark-results.json
```

El ejecutable Rust `benchmark_matrix` usa `release`, fixtures sintéticas y una
matriz declarada en código antes de las mediciones. Son 14 puntos, variando un
factor a la vez respecto de (32 nodos, 32 aristas, 4 invariantes, 1 variable por
invariante, 8 átomos delta). Se varían nodos 16/32/128/512, aristas 0/32/256,
invariantes 1/4/16, variables 0/1/2/3 y delta 0/8/32/64 átomos. Cada sustitución
de etiqueta cuenta dos átomos: retiro y alta.

Hay un calentamiento no registrado por punto y cinco muestras completas. Se
mide `proponer`, incluyendo construcción del candidato y evaluación, con la
misma cuota semántica de producción. Quedan fuera construcción de fixtures,
parseo, I/O de archivos, arranque de procesos e inferencia LLM. Los invariantes
son tautologías cuantificadas para recorrer dominios sin terminar por encontrar
una contradicción temprana. Los hashes de fuentes, versión rustc, perfil,
arquitectura y número de CPU lógicas acompañan las muestras crudas; se reportan
mínimo, mediana y máximo, sin percentiles extrapolados de cinco valores.

**Resultado adverso conservado:** los cinco intentos con tres variables agotan
el presupuesto de evaluación. Esos tiempos son tiempo hasta el error, no tiempo
de admisión. Los otros 65 intentos admiten. No se reduce la carga ni aumenta la
cuota para ocultar el límite. Cada punto incluye sus conteos de outcomes.

Los resultados son de un solo host y no constituyen SLA, cota RSS ni una medida
del costo monetario de un agente. Los costos/uso del modelo pertenecen al piloto
pareado y se registran por separado.

## Identidad del código medido

El campo `environment.git_head` del benchmark conserva `79f1baa`, el HEAD que
existía al medir mientras las fuentes nuevas aún no estaban comprometidas. No
significa que el benchmark estuviera incluido en ese commit. Los campos
`source_sha256` identifican el script y ejemplo Rust efectivamente ejecutados;
el checkpoint posterior `e076e74` conserva esas fuentes prerregistradas. El
corpus formal también identifica sus fuentes y etiquetas por hash. No se atribuye
una medición a un árbol limpio distinto del que realmente se ejecutó.
