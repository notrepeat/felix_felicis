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
atómico frente a mutaciones hostiles, visibilidad continua del root ni durabilidad
ante pérdida de energía/caída del sistema. Windows identifica volumen y Linux
usa la identidad de montaje donde el backend la ofrece; ninguna afirmación de
esta página sustituye esas comprobaciones por comparaciones de rutas.

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
