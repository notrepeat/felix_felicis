# Proponer, aplicar y recuperar una transacción

`universo propose` simula por defecto. Lee el delta JSON v1, captura el snapshot
y evalúa el candidato sin escribir. `--emit-snapshot-token` entrega el token
sensible para una aplicación posterior; no es una autorización independiente.

```text
universo propose <raiz> --manifiesto <m.toml> --invariantes <i.sx> --delta <d.json> --format json --emit-snapshot-token
universo propose <raiz> --manifiesto <m.toml> --invariantes <i.sx> --delta <d.json> --format json --apply --expect-snapshot <token>
```

El JSON usa `command: "propose"`, `mode: "simulate"|"apply"`, `status:
"admit"|"reject"|"error"`, contadores de conflictos/invariantes fallidas y
códigos de error redactados. Un objeto sale por stdout; stderr queda vacío
salvo uso inválido o fallo de escritura. Los códigos de salida son 0 para
admit, 1 para reject/error y 2 para uso inválido. Apply no refleja el token de
entrada ni admite `--emit-snapshot-token`. El texto mantiene la misma privacidad.

La API Rust es `universo_okf::transaccion::aplicar::aplicar_archivo(raiz,
manifiesto, invariantes, delta, expected_snapshot)`. Devuelve `Aplicada` o
`Rechazada` con contadores; los errores exponen sólo un código y si conservan
evidencia. El tipo de error posee esa evidencia y no concede una mutación.
No hay retry automático después de un error ambiguo. Mientras el caller
retiene un error, sus handles pueden impedir otro intento en Windows; el
caller decide cuándo descartar esa evidencia.

## Qué ocurre bajo el lock

1. La ubicación de root se valida sin aceptar symlinks/junctions/reparse. El
   delta y las fuentes externas de configuración se leen con cuotas antes
   de adquirir el lock de escritor.
2. Se adquiere el lock sibling reutilizable y se recupera el estado reconocido
   antes del CAS. La configuración interna se resuelve después de recovery
   desde el inventario anclado: también funciona si root faltaba por un corte
   después de moverlo a backup. Esta lectura interna es deliberadamente bajo
   lock; no se presenta como lectura previa al lock.
3. Se compara el token antes de parsear la configuración y se vuelve a evaluar
   la propuesta. `stale` rechaza un estado distinto. Un rechazo conserva los
   archivos del root; el lockfile reutilizable puede haberse creado y recovery
   puede haber completado una operación anterior antes de ese rechazo.
4. Un candidato admitido se serializa a un stage exclusivo, preservando bytes
   auxiliares regulares y directorios vacíos. Se recarga y compara el candidato.
5. Se publica Context v2 y Journal v2 Prepared, se mueve root Old a backup,
   se publica BackupReady, se mueve stage New a root y se publica Installed.
6. Se repite replay y se confirma Committed antes de limpiar backup. Intent v3
   conserva Context original para retirar Journal, Context y finalmente Intent.
   El resultado exitoso conserva root y el lockfile, sin artefactos transaccionales.

El guard permanece desde recovery hasta cleanup. Dos escritores con el mismo
token no producen last-write-wins: uno aplica y el otro recibe `writer_lock_busy`
o `stale`. El lock coordina escritores Felix cooperantes. No se promete snapshot
atómico frente a mutaciones hostiles ni visibilidad continua del root. Windows identifica volumen y Linux
usa la identidad de montaje donde el backend la ofrece; ninguna afirmación de
esta página sustituye esas comprobaciones por comparaciones de rutas.

## Durabilidad ante caída del sistema

Desde 2026-11-17 el apply hace `fsync` en un orden fijo (`transaccion/durabilidad.rs`):
cada archivo del stage y cada directorio del stage, del más profundo al stage,
antes de publicar Journal Prepared; y el directorio padre después de cada cambio
de nombres transaccional (Context, Journal y sus sucesores, Intent, root→backup,
stage→root, borrado de backup, retiros y limpieza de stage). Así el backup sólo
se borra después de que el sucesor Committed sea durable. Un fallo de `fsync`
tras un rename ya hecho se informa como resultado no verificado y queda para
recovery. En los pasos que confirman por observación (sucesor del Journal,
Intent), el `fsync` se hace después de confirmar y su fallo deja el resultado
como indeterminado: el apply no borra el backup y `recover` lo resuelve. Dos
tests comprueban el orden y ese fallo inyectado; no hay prueba real de pérdida
de energía. Residuo conocido: en los retiros finales (Journal, Context, Intent)
el resultado del `unlink` y de su `fsync` no se usa, porque el protocolo confirma
por observación y es reanudable por fases; un `fsync` fallido ahí no se informa.

Límite en Windows: los archivos se vacían con `FlushFileBuffers`, pero las
entradas de directorio no se sincronizan explícitamente (los handles de
directorio no tienen derecho de escritura); se confía en el journal de
metadatos de NTFS.

## Después de una interrupción

Otra llamada a apply inspecciona el Journal/Context/Intent desde disco bajo el
lock antes de comprobar el token. En estados reconocidos Prepared/BackupReady
recupera Old; en Installed conserva New sólo con replay y huella válidos.
Committed permite terminar backup parcial con la identidad Old exacta. El retiro
v3 conserva evidencia hasta verificar el estado final. En rollback se retira
Context y después Journal; en commit se retira backup, luego Journal/Context y
finalmente Intent. La tabla legacy v1 permanece separada.

Una configuración externa se captura antes del lock; una interna se recupera
junto al root y se captura después. Si recovery concluyó una aplicación previa,
el token recibido puede resultar stale: simula de nuevo para decidir la próxima
propuesta. Un temporal huérfano, stage ajeno, mezcla de versiones, enlace o
identidad no verificable bloquea el flujo y conserva los artefactos. No se deduce
permiso para eliminar un archivo a partir de su nombre.

Si la preparación falla después de crear el stage (por ejemplo, `stale` porque
el root cambió durante la preparación), apply lo retira en el mismo lock con la
capability anclada que lo creó y devuelve el código original. Si no puede
(owner invalidado o borrado no verificable), conserva la evidencia y devuelve
`apply_stage_retained`. Un stage sin capability ni journal (proceso muerto antes
del journal) nunca se borra automáticamente.

## Notas nuevas (`felix-delta-v2`)

Un delta v2 puede crear notas. El núcleo las inserta en una base antes de
resolver el resto del delta; el stage escribe su Markdown (frontmatter `type` y
`title`, cuerpo opcional) y la recarga del stage debe reproducir exactamente el
candidato. El contexto de admisión guarda el delta en forma canónica: v1 si no
crea notas (idéntico al formato anterior) y v2 si las crea, de modo que el
recovery reproduce el mismo candidato; un motor anterior a v2 falla cerrado al
leer un contexto v2. Un test corta el apply de una nota en cada paso de la
instalación y comprueba que `recover` deja el estado viejo o el nuevo completo.

Un delta v2 también puede llevar `update` (cuerpos nuevos para notas
existentes, conservando su frontmatter no mapeado); `universo delta` lo genera
desde un borrador editado. El test de cortes del apply incluye una
actualización de cuerpo junto con la nota nueva.

Una arista se escribe como `[[stem]]` de su destino. Si otra nota comparte ese
nombre, el cargador la vería ambigua al recargar; por eso `propose` responde
`error` con `edge_target_ambiguous` (no `reject`: el delta no se puede
representar, no incumple una regla) en la simulación y en el apply, con el
mismo criterio del cargador (nombre exacto, sólo el destino). Dos notas con el
mismo nombre sin aristas hacia ellas siguen siendo válidas.

## Sello e historial

Tras `Committed`, bajo el mismo lock, apply escribe el sello del estado admitido
(ver [guardia](guardia.md)) y añade una línea a `.<raiz>.felix.log` con fecha
UTC, autor y motivo (opcionales, `--author`/`--reason`, una línea, sin
verificación de identidad, sin caracteres de control ni invisibles o bidi) y si
quedó sellado. `universo log` la muestra sin el token del estado; las líneas
ilegibles o fuera de formato (escritura interrumpida, edición manual) se
descartan, se cuentan en `discarded_lines` y no se imprimen. El historial nunca
se escribe a través de un enlace ni en algo que no sea un archivo regular, y una
línea parcial previa se cierra antes de añadir la siguiente. Es informativo: no
participa en el recovery y un fallo al escribirlo no deshace el apply
(`"logged": false`; la línea puede haber quedado a medias). Tras un apply quedan,
por tanto, tres siblings persistentes: lockfile, sello e historial.

## Diagnóstico y recover

```text
universo status <raiz> [--format text|json]
universo recover <raiz> [--format text|json]
```

`status` no toma el lock ni escribe: lista los siblings reservados
`.<raiz>.felix.*` y responde con conteos y presencia (root, journal, context,
intent de retiro, backup, stages, otros reservados), sin nombres ni rutas. Sale
con 0 si no hay nada pendiente y 1 si lo hay; `missing` indica que el root no
existe y no hay rastro transaccional (ruta equivocada), y `recover` responde
entonces `recover_root_absent` sin crear el lockfile. Con un stage sin journal añade
`orphan_stage: true` y la instrucción para revisarlo y borrarlo a mano si no hay
otro escritor activo. `recover` toma el lock y ejecuta el mismo recovery que
precede a cada apply, sin aplicar nada; ante un estado no reconocido falla con
`recovery_pending_unverified` y conserva todo. `check` no toma el lock: si
detecta un estado pendiente, lo avisa por stderr sin cambiar stdout ni el código
de salida.

El delta tiene una cuota de 16 MiB antes de parsear. Las dos fuentes de
configuración comparten 128 MiB lógicos, incluso cuando son la misma fuente;
las externas se acotan durante lectura y las internas heredan la cuota del
inventario anclado y la cuota combinada. El árbol raw y la limpieza tienen
sus propios límites de entradas, profundidad y rutas. Esto no es una promesa
de cota total de RSS ni de tiempo del parser.

## Ejemplo y comprobación

```text
cargo build --locked -p universo-cli
python tools/demo_apply.py --binary target/debug/universo
```

En Windows usa `target/debug/universo.exe`. El ejemplo crea un temporal,
obtiene un token, aplica un cambio Type y comprueba bytes auxiliares y ausencia
de restos sin imprimir el token. Las pruebas también terminan un escritor en
cuatro puntos y lanzan otro proceso que recarga evidencia del disco y ejecuta
el dispatcher. No equivalen a una prueba de power-loss.

Las [mediciones de dos escritores](04-mediciones-apply.md) documentan las
muestras reproducibles y la decisión de mantener validación completa y replay.
