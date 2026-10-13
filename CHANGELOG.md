# Changelog

Los cambios se agrupan por release. La sección `Unreleased` es una propuesta
para el próximo gate `0.2.0`; aún no hay tag ni actualización de versiones.

## Unreleased — candidato a 0.2.0

### Presupuesto de evaluación de propuestas

- `proponer` comparte 1.000.000 de unidades deterministas de evaluación entre
  todas las fórmulas expandidas de una propuesta. El contador empieza después
  de construir el candidato, detectar conflictos y completar la expansión;
  esta última conserva su cuota separada de 100.000 instancias.
- Si se agota el presupuesto, la propuesta falla con
  `evaluation_budget_exhausted` y no devuelve un candidato ni un resultado
  parcial. `evaluar` conserva su presupuesto por llamada y el `check` CLI
  baseline lo reinicia por fórmula.
- El límite cubre unidades de evaluación, no el trabajo previo, las copias,
  memoria o tiempo total.

### Tokens de snapshot

- El endurecimiento pre-release hace que `calcular_token_snapshot` rechace
  rutas lógicas no representables y duplicados exactos; los tokens v1 de
  entradas válidas conservan sus bytes.
- `calcular_token_snapshot` y `calcular_digest_raw_tree` aplican antes de
  ordenar cuotas inclusivas al árbol raw: 100.000 entradas, 128 MiB de
  contenido raw, profundidad 128 y 16 MiB de rutas UTF-8 acumuladas. Exceder
  una cuota devuelve `snapshot_resource_limit`. `calcular_token_snapshot`
  también aplica, antes de validar u ordenar el árbol y de iniciar el hashing,
  una cuota inclusiva independiente de 128 MiB para la suma de
  `manifiesto_raw` e `invariantes_raw`; esta cuota no se suma a la del árbol.
  `version_motor` y `policy_id` aún no tienen cota. Los tokens v1 dentro de
  cuota conservan sus bytes. Este chequeo de slices ya recibidas no garantiza
  un límite de memoria total ni del parser.
- La captura privada por ruta limita durante la lectura la suma de
  `manifiesto_raw` e `invariantes_raw` a una cuota inclusiva de 128 MiB,
  independiente de la cuota del árbol raw. Reutiliza los buffers de archivos
  ya inventariados bajo root; para un archivo externo limita la lectura al
  remanente más un byte centinela. La misma ruta externa absoluta normalizada
  se lee una vez, aunque sus bytes se cobran por cada campo que la referencia.
  No deduplica por identidad física ni garantiza un límite de RSS o del parser;
  la captura por ruta conserva TOCTOU y no garantiza atomicidad ante cambios
  hostiles del filesystem. `version_motor` y `policy_id` siguen sin cota.
- La captura anclada valida esa suma combinada antes de abrir el root bajo lock
  y antes de inventariarlo. Si excede la cuota, devuelve el error redactado
  `snapshot_resource_limit`, conserva intacto el root/canary y retiene el guard
  recibido. Dentro de cuota, el CAS sigue ocurriendo antes de provenance y
  parseo. Desde `4b94261`, `ConfigRawCapturada::nueva` también es fallible y
  rechaza una suma fuera de cuota como invariante de construcción. La comprobación
  ocurre después de convertir ambas entradas a `Arc<[u8]>`: no limita lectura,
  asignaciones previas, RSS ni I/O antes del lock. Cuenta por campo lógico, así
  que dos campos que comparten el mismo `Arc` se cobran dos veces. El preflight
  del consumidor sigue siendo una defensa en el límite de uso. Aún faltan el
  proveedor acotado antes del lock y sus callers de apply/CLI, además de cotas
  para `version_motor` y `policy_id`; no se afirma atomicidad ante cambios
  hostiles.
- El CAS privado por ruta compara el token después de capturar también la
  configuración externa. Las regresiones verifican que un match conserva los
  nombres, tipos y bytes del árbol y los bytes externos, no crea un lockfile
  sibling y funciona con el guard del caller retenido (`writer_lock_busy` para
  un segundo escritor). Si cambió la configuración externa, devuelve `stale`
  redactado antes de parsear, sin cambios de contenido o namespace. La prueba
  añadida en `a3cc3c9` conserva además un journal sibling y compara los
  inventarios observados de root, configuración y parent, incluidos los bytes
  del journal, tanto en match como en stale. También compara el
  `ArtifactFingerprint` del root (identidad del directorio raíz y digest raw),
  pero no identidades individuales de archivos o siblings. La ruta no hace
  escrituras intencionales de contenido o namespace; las pruebas no cubren
  atime, permisos ni otros metadatos. Se conserva TOCTOU y no se garantiza
  atomicidad ante cambios hostiles; el helper sigue privado, sin caller desde
  apply/CLI ni simulación pública.

### Evidencia interna de journal y recuperación

- `eef28f5` amplía la regresión privada del dispatcher a seis combinaciones de
  `KeepOld` y `RestoreOld` en `Prepared`, `BackupReady` e `Installed`, con una
  observación inicial por ejecución. `51cd5c4` añade una comparación recursiva
  del parent: rutas, tipos y bytes de archivos normales; para el lock abierto
  solo comprueba presencia, tipo y longitud. En `RestoreOld`, el inventario
  esperado mueve el subárbol de backup a root y retira el journal. Esta
  cobertura no integra el dispatcher en apply/producción ni completa U7, que
  sigue parcial. El inventario no comprueba atime, permisos ni otros
  metadatos; no elimina TOCTOU ni demuestra atomicidad ante cambios hostiles.
- Matriz workspace independiente sobre `51cd5c4`: Windows, 702 passed/0
  failed/6 ignored en 37 suites; WSL Ubuntu, 710 passed/0 failed/6 ignored en
  37 suites. `fmt` y `clippy` pasan en ambos entornos. Estos son conteos del
  workspace, no resultados focales de las pruebas de U7 o U8.

### Límites y seguridad de carga

- `cargar_bundle` limita la lectura desde disco a 128 MiB de contenido Markdown,
  100.000 entradas, profundidad 128 y 16 MiB de rutas lógicas acumuladas. Las
  cuotas son inclusivas y globales a todas las raíces declaradas.
- `cargar_bundle` limita además a 16 MiB las rutas canónicas físicas relativas
  que retiene en el índice de raíces y en `procesados`. Cada copia retenida
  cuenta; un alias de diamante omitido no se vuelve a cobrar.
- Si una entrada o symlink no Markdown no se puede clasificar o recorrer de
  forma segura, la carga falla en vez de omitirla y devolver un bundle parcial.
- `parsear_bundle` recibe documentos ya materializados y queda fuera de las
  cuotas de `cargar_bundle`.

### Compatibilidad pre-1.0

- Se agregan variantes públicas a enums exhaustivos: `ErrorPath::{SegmentoTerminaEnPunto,
  SegmentoTerminaEnEspacio, DispositivoReservado, CaracterNoPortable,
  SegmentoDemasiadoLargo { limite_utf16 }}`;
  `ErrorManifiesto::{DirectoriosDuplicados, DirectoriosSolapados,
  DirectoriosRutaInvalida, SinIotaRutaInvalida, IgnoradosRutaInvalida}`;
  `ErrorEstructura::HijoDuplicadoEnLote`; `ErrorSnapshot::LimiteInventarioExcedido`
  y `Regla::LimiteBundleExcedido`. Los consumidores con `match` exhaustivos
  deben contemplar estas variantes.
- `BundlePath` y las rutas de `layout.directorios`/`layout.sin_iota` exigen
  segmentos portables a Windows; `layout.ignorados` exige un nombre de un solo
  segmento portable. En las rutas de layout cada componente físico admite
  hasta 255 unidades UTF-16. En `BundlePath`, los componentes intermedios
  admiten 255 y el último stem admite 248 para dejar espacio a `.md.tmp`.
  `ErrorPath::SegmentoDemasiadoLargo` informa el límite aplicable. Se rechazan
  caracteres Windows no admitidos, puntos o espacios finales y dispositivos
  DOS reservados (incluidos `COM¹`–`COM³` y `LPT¹`–`LPT³`). Acorta los nombres
  que excedan el límite antes de migrar.
- Renombra los archivos y directorios que incumplan esas reglas y actualiza
  `layout.directorios`, `layout.sin_iota`, `layout.ignorados`, los wikilinks y
  cualquier referencia `Parent` explícita que apunte a las rutas renombradas.
- `layout.directorios` exige rutas disjuntas: no se admiten declaraciones
  iguales ni rutas que sean ancestro/descendiente entre sí; por ejemplo, `Docs`
  y `docs/sub` se solapan. La clave Unicode completa en mayúsculas es
  conservadora: colapsa `Σ`/`ς` y `ᾀ`/`ᾈ`, y puede rechazar expansiones como
  `ß`/`SS`. No afirma equivalencia exacta con las reglas de Windows; el staging
  en Windows compara rutas físicas con `CompareStringOrdinal`. También se
  rechazan raíces que resuelvan por symlink/junction al mismo directorio o a
  directorios físicamente solapados.
- Para errores, `regla` y `path` sirven para clasificar y localizar. El campo
  `detalle` es texto humano no estable y no debe analizarse programáticamente;
  si se exceden varias cuotas a la vez, cuál se reporta primero tampoco es un
  contrato estable.

Las versiones de los crates permanecen en `0.1.0` hasta cerrar el gate de
release y publicar la migración a `0.2.0`.
