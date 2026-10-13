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

### Evidencia parcial de fase 05: smoke CLI, fixture y runner de arquitectura

- `ae99906` incorpora un smoke de la CLI sobre las fixtures públicas de
  admisión y rechazo; `c76e292` aclara que `check` es evaluación estática y
  precisa el contrato de JSON y streams. La matriz local sobre `c76e292` pasa
  en Windows y WSL Ubuntu: respectivamente, el workspace registra 702 y 710
  pruebas `passed`, 0 `failed` y 6 `ignored` en 37 suites. `fmt`, `clippy` y
  la frontera pública también pasan en ambos entornos.
- Cada smoke compila la CLI y realiza dos invocaciones directas, una de
  admisión y otra de rechazo: cuatro invocaciones CLI en total entre ambos
  sistemas operativos; no son pruebas Cargo. El caso admit devuelve código 0
  y resumen `sources/instances/evaluated/failed = 3/3/3/0`; reject devuelve
  código 1 y `1/1/1/1`. Ambos validan JSON v2, los streams, una allowlist de
  estructura/valores y la ausencia de los marcadores conocidos de
  rutas/etiquetas/textos/IDs de la fixture.
- `2534fd4` añade el test de integración `demo_arquitectura`, que ejecuta un
  flujo sintético en memoria; `e0fc2e6` refuerza sus aserciones. La matriz
  local independiente sobre `e0fc2e6` pasa en Windows y WSL Ubuntu: prueba
  focal `demo_arquitectura`, 1 passed/0 failed/0 ignored en ambos; smoke CLI
  admit/reject pasa en ambos; `cargo test --workspace`
  registra Windows 703 passed/0 failed/6 ignored (38 suites) y WSL 711/0/6
  (38 suites). `fmt`, `clippy` y la frontera pública pasan en ambos. No se
  registra un resultado de CI remoto para esta matriz.
- El test admite el delta vacío y verifica que los átomos y anexos del bundle
  base permanecen intactos. `proponer` rechaza el delta domain→adapter con un
  testigo en fuente 0, línea 2, columna 1; vuelve a comprobar el bundle base
  tras el rechazo y tras construir en memoria el candidato contradictorio. En
  esa versión, el test construye explícitamente el delta correctivo
  domain→port→adapter y Felix lo admite. Solo se serializa y recarga el
  candidato corregido; se comparan átomos y anexos, y la segunda serialización
  conserva los bytes de los tres Markdown. `validate` y `check` se ejecutan
  solo sobre ese candidato corregido: 3 nodos, 2 aristas y `failed=0`.
- `8fe29c1` incorpora la fixture pública durable en
  `examples/software-architecture/`; `434f4fc` cambia
  `demo_arquitectura` para cargar de ella el bundle, manifiesto, invariante y
  deltas contradictorio y correctivo. La matriz local sobre `434f4fc` pasa en
  Windows y WSL Ubuntu: la prueba focal registra 1 passed/0 failed/0 ignored
  en ambos, y el smoke CLI admit/reject pasa en ambos. `cargo test --workspace`
  registra Windows 703 passed/0 failed/6 ignored (38 suites) y WSL Ubuntu
  711 passed/0 failed/6 ignored (38 suites); `fmt`, `clippy` y la frontera
  pública pasan en ambos. `proponer` rechaza domain→adapter en core con
  testigo fuente 0, línea 2, columna 1; el test construye en memoria el
  candidato contradictorio y la fixture aporta la corrección explícita
  domain→port→adapter. Solo serializa y recarga el candidato corregido; la
  CLI `validate` y `check` se ejecutan sobre ese corregido (3 nodos, 2 aristas,
  `failed=0`).
- `5510329` hace que `demo_arquitectura` cargue los snapshots durables
  `states/contradictory` y `states/corrected` y compare sus átomos y anexos con
  los candidatos calculados desde la fixture. La matriz local sobre HEAD
  `55103297`: Windows pasa la prueba focal (1 passed/0 failed/0 ignored), el
  smoke CLI admit/reject, el workspace (703 passed/0 failed/6 ignored, 38
  suites), `fmt`, `clippy` y la frontera pública. WSL Ubuntu pasa las mismas
  comprobaciones, con workspace 711/0/6 (38 suites); usa un checkout aislado
  idéntico byte a byte de 184 archivos y 5.145.096 bytes. No se comprobó CI remoto.
  Los snapshots son datos ilustrativos versionados de la fixture, no salidas
  de `apply` ni correcciones generadas. El candidato contradictorio reconstruido
  durante la prueba sigue solo en memoria; solo el corregido se serializa y
  recarga, y solo sobre él corren CLI `validate` y `check`.
- Al cierre de `5510329`, la evidencia seguía siendo parcial: el test consumía
  la fixture durable y contrastaba sus snapshots, pero aún no había runner
  autónomo ni goldens. El candidato contradictorio calculado en runtime seguía
  en memoria; el snapshot no provenía de `apply`. Esa versión tampoco probaba
  rechazo por CLI, generación automatizada de la corrección, `apply`,
  publicación, atomicidad o recovery.
- `71784b0` añade 12 goldens de texto y JSON para `validate` y `check` en los
  estados base, contradictorio y corregido. `179b5f7` agrega
  `tools/run_software_architecture_demo.py`, que compila la CLI y compara esas
  12 salidas byte por byte; `e857d1c` valida además el esquema de los goldens
  JSON y la forma del testigo abstracto. El runner inspecciona los tres estados
  versionados; no reproduce la secuencia de deltas. El test Rust separado
  cubre el rechazo por `proponer`, la corrección explícita y el round-trip del
  candidato corregido, pero no usa `apply`.
- Matriz local sobre `e857d1c`: Windows y WSL Ubuntu pasan las 12 comparaciones
  exactas del runner, la prueba focal `demo_arquitectura` (1 passed/0 failed/0
  ignored), `cargo test --locked --workspace` (Windows 703 passed/0 failed/6 ignored;
  WSL 711/0/6, 38 suites en ambos), `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` y
  `python tools/check_public_boundary.py`. Los checkouts aislados son idénticos
  byte por byte: 196 archivos y 2.429.126 bytes; no hubo drift en los goldens.
  `.github/workflows/ci.yml` ejecuta el runner en ambos jobs, pero aún no hay
  resultado de CI remoto para `e857d1c`.
- Al cierre de `e857d1c`, la fase 05 permanecía pendiente: el testigo solo
  señalaba posición de fuente y predicados, no nombres de archivo ni
  IDs/argumentos de átomos; la fixture aún no incluía un caso de ausencia; y
  el runner no medía duración ni RSS. Tampoco demostraba `apply`, publicación,
  atomicidad o recovery.
- `3b05f42` añade un caso de ausencia por falta de testigo existencial: el
  delta retira la arista `domain→port` y su átomo `HasLab`, dejando tres nodos
  y una arista. La prueba Rust verifica la correspondencia entre esa operación,
  el snapshot y el testigo abstracto, sin exponer los valores concretos.
- `fab86f8` amplía el runner de tres estados y 12 salidas a cuatro estados
  (base, contradictorio, corregido y gap) y 16 salidas exactas. Valida la forma
  del testigo de gap y el presupuesto, rechaza claves JSON duplicadas y busca
  marcadores concretos de la fixture en `stdout`, `stderr` y los goldens. El
  testigo público sigue siendo abstracto y redactado.
- Matriz local aislada sobre `fab86f8`: Windows y WSL Ubuntu pasan el smoke CLI
  público, el runner (16/16 salidas exactas), la prueba focal
  `demo_arquitectura` (1 passed/0 failed/0 ignored), `cargo test --locked
  --workspace` (Windows 703/0/6 y WSL 711/0/6; 38 suites en ambos),
  `cargo fmt --all -- --check`, `cargo clippy --locked --workspace
  --all-targets -- -D warnings` y `python tools/check_public_boundary.py`.
  Los checkouts aislados son idénticos: 205 archivos y 2.451.333 bytes; los 16
  goldens LF no cambiaron, con SHA-256
  `1560c1995381ff042502a657d703d3c42523acaf0c3f91c692764218a0523faf` y hash
  canónico del árbol `bfd293225bddfd418aa6dfd1146525645ec0a72a0f7e0040f494089ea50a1781`.
  No se comprobó CI remoto para este SHA.
- La fase 05 sigue pendiente bajo su política integral: el runner compara
  snapshots, pero no reproduce los deltas; el test verifica la correspondencia
  concreta solo internamente, mientras el testigo público no señala archivos
  ni átomos concretos. No hay medición de duración o RSS ni demostración de
  `apply`, publicación, atomicidad o recovery.

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
