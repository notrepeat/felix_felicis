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
- En el corte de `86bb79b` (antes de `c888347`/`c763f85`), `34b7e0a`
  clasifica `PartialNew` cuando coincide la identidad
  durable del fingerprint `New` esperado por el journal y difiere el digest
  raw; una identidad distinta sigue siendo `Unknown`. `428d432` amplía `KeepOld` en `Prepared`,
  `BackupReady` e `Installed` para root `Old`, backup `Absent`, stage `New` o
  `PartialNew` y `root_new_validated=false`. Reabre el stage anclado por
  identidad durable, exige digest completo para `New`, limpia y reobserva antes
  de retirar el journal. Ante cleanup ambiguo, la reobservación permite retirar
  el journal si el stage ya desapareció; si queda parcial, se conserva el
  journal. En un intento posterior, `PartialNew` permite reintentar con la
  misma identidad aunque el digest haya cambiado por la limpieza parcial.
- `980a3f0` agrega cobertura para cleanup ambiguo, reintento parcial después
  de soltar y readquirir el lock,
  identidad distinta y txn inválido; `86bb79b` corrige dos préstamos en las
  aserciones de recovery. Los cuatro tests visibles son
  `stage_presente_falla_cerrado_y_conserva_journal_y_root`,
  `cleanup_ambiguo_reobserva_y_retira_journal_solo_despues_del_stage`,
  `txn_invalido_falla_antes_de_mutar_siblings_y_journal` y
  `keep_old_reintenta_limpieza_parcial_del_stage_tras_reinicio`. Los tests cuyo
  nombre alude a reinicio sueltan y readquieren el lock en el mismo proceso; no
  matan ni relanzan el proceso y no prueban crash durability.
- CI run `38504988001` pasó tests y fmt, pero clippy falló por los dos
  préstamos corregidos en `86bb79b`. El CI final `38505451293` terminó con
  éxito en `86bb79b809625847ed4835385a31068fc97e363e`: Linux 716 passed/0
  failed/6 ignored, Windows 708/0/6, 38 resúmenes de suites; el unittest de
  herramientas pasó 7 casos por sistema operativo. `fmt`, clippy y la frontera
  pública también pasaron.
- Al cierre de `86bb79b`, U7 seguía parcial y sin caller desde apply/CLI ni
  publicación completa. `RestoreOld` aún requería stage `Absent`;
  `DiscardNewRestoreOld`, `KeepValidated`, `Committed` y recovery completo
  seguían pendientes. No se afirmaba atomicidad ni durabilidad: el lock
  coordina escritores Felix cooperantes y TOCTOU por mutaciones externas queda
  fuera del contrato.
- 2026-10-14, `c888347`: `RestoreOld` acepta en `Prepared`, `BackupReady` e
  `Installed` root `Absent`, backup `Old`, `root_new_validated=false` y stage
  `Absent`, `New` o `PartialNew`. Reabre por identidad durable; exige digest
  completo para `New` y admite `PartialNew` cuando coincide la identidad y el
  digest raw difiere. Limpia y reobserva antes de restaurar el backup y retirar
  el journal al final. Si cleanup reporta error, continúa solo cuando la nueva
  observación confirma que el stage desapareció; si queda parcial, conserva el
  journal para un nuevo intento tras readquirir el lock. Prueba focal
  `cargo test -p universo-okf recovery::`: 60 passed.
- 2026-10-14, `c763f85`: añade `DiscardNewRestoreOld` solo para `BackupReady`
  con root `New`, backup `Old`, stage `Absent` y validación false o true, y
  `Installed` con esa fila y validación false. Renombra root al stage
  identificado por el journal y reobserva para reconciliar un error ambiguo;
  luego limpia/reobserva, restaura el backup y retira el journal al final. Un
  root `New` junto a cualquier stage sibling falla cerrado. Tras cleanup
  parcial, una nueva invocación de `RestoreOld` puede reintentar por identidad
  durable. Foco
  `cargo test -p universo-okf recovery::`: 65 passed. En conjunto, ocho
  pruebas/casos de recovery nuevos visibles cubren las filas, la ambigüedad,
  el retry y el rechazo con canarios intactos.
  Los retries descritos como "tras reinicio" se simulan soltando y readquiriendo
  el lock en el mismo proceso; no incluyen kill/restart ni prueban crash
  durability.
- CI run `38509652854` terminó con éxito para
  `c763f8527d412778e99eb26c81194f96fdc0c545`: Linux 723 passed/0 failed/6
  ignored, Windows 715/0/6, con 38 resúmenes de suites. El unittest de
  herramientas pasó 7 pruebas por sistema operativo; `fmt`, clippy y
  `check_public_boundary.py` también pasaron. U7 permanece parcial, sin caller
  desde apply/producción ni publicación completa; `KeepValidated`, `Committed`
  y el recovery completo siguen pendientes. No se afirma atomicidad ni
  durabilidad: el lock coordina solo escritores Felix cooperantes y TOCTOU
  por mutaciones externas queda fuera del contrato. Runtime harness: N/A; las
  pruebas focales ejercitan filesystem real y rename ambiguo inyectado. Esta
  actualización documental no agrega prueba focal ni runtime harness; ambos
  son N/A. Solo toca `CHANGELOG.md` y los dos planes, y su rollback queda
  limitado a esos archivos. `git diff --check` y
  `python tools/check_public_boundary.py` pasan tras editar.

### Fase 05 — historial de evidencia y cierre de la demo pública

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
  con 184 archivos. No se comprobó CI remoto.
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
  `python tools/check_public_boundary.py`. La comparación incluyó 196 archivos;
  no hubo drift en los goldens.
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
  La comparación de SHA-256 por archivo confirma que los 205 archivos de los
  checkouts Windows y WSL son byte-idénticos; los 16 goldens LF permanecieron
  iguales y pasaron las comparaciones exactas. No se comprobó CI remoto para
  este SHA.
- Al cierre de `fab86f8`, la fase 05 seguía pendiente bajo su política
  integral: el runner comparaba snapshots, pero no reproducía los deltas; el
  test verificaba la correspondencia concreta solo internamente, mientras el
  testigo público no señalaba archivos ni átomos concretos. No había medición
  de duración o RSS ni demostración de `apply`, publicación, atomicidad o
  recovery.
- `f15af52` compone el runner en un solo comando: primero ejecuta
  `cargo test --locked -p universo-cli --test demo_arquitectura --quiet` y,
  si pasa, compila la CLI y compara cuatro estados (base, contradictorio,
  corregido y gap) con 16 salidas `validate`/`check` en JSON y texto. La prueba
  de integración recorre rechazo, corrección y gap con APIs Rust; reconstruye
  candidatos en memoria y los compara con snapshots versionados. No usa CLI
  `apply` ni genera la corrección.
- Matriz local aislada sobre `f15af52`: Windows y WSL Ubuntu pasan el runner
  (16/16), la prueba focal (1 passed/0 failed/0 ignored) y el smoke CLI. El
  workspace registra Windows 703 passed/0 failed/6 ignored y WSL Ubuntu
  711/0/6 (38 suites en ambos). `cargo fmt --all -- --check`,
  `cargo clippy --locked --workspace --all-targets -- -D warnings` y
  `python tools/check_public_boundary.py` pasan en ambos. Los 205 archivos
  comparados son byte-idénticos; los 16 goldens LF no presentan drift. WSL
  Ubuntu invoca el runner con `python3`. El workflow ya ejecuta el runner en
  Linux y Windows, pero no hay CI remoto para este SHA.
- Al cierre histórico de `f15af52`, la ruta compuesta aportaba cobertura
  técnica para los criterios 1, 2 y 4 y el workflow cableaba el criterio 5,
  pero las seis casillas seguían abiertas por falta de correlación test-only
  del testigo con archivos/átomos concretos, mediciones y CI remoto para ese
  SHA. Entonces el progreso maestro era 3/9.
- `68353e0` registra duración monotónica y RSS muestreado; `1043b1b` mantiene
  compatible el tipado de métricas con Python 3.7. `be3641a` correlaciona
  dentro del test los testigos redactados con los archivos y átomos concretos
  de la fixture y comprueba la redacción de la salida pública. `3f20723`
  acota las coincidencias de tokens de fixture en la comprobación de redacción.
- El CI remoto `38500576872` terminó correctamente en Linux y Windows para el
  cierre en `3f20723`. Sus métricas reportadas son:

  | Sistema | `elapsed_ns` | `rss_tree_peak_sampled_bytes` |
  |---|---:|---:|
  | Linux | 5689835724 | 1357873152 B |
  | Windows | 15457254600 | 919347200 B |

  La memoria es una estimación muestreada cada 100 ms: puede perder picos entre
  muestras y sumar páginas compartidas más de una vez. Linux reporta RSS y
  Windows working set; no son valores exactos ni directamente comparables.
  Las métricas no son umbrales de CI.
- Fase 05 queda completa y el progreso maestro pasa a 4/9. Las seis casillas
  se cierran con el runner de un comando, sus 16 comparaciones golden, el test
  de integración, las mediciones y CI en ambos sistemas. La correlación con
  fixture concreta ocurre solo dentro del test; `stdout`, `stderr` y los
  goldens permanecen redactados. La prueba no usa CLI `apply` ni genera la
  corrección: esta procede del delta explícito de la fixture. Solo el candidato
  corregido se serializa y recarga, y snapshots y goldens no son salidas de
  `apply`. No se demuestra publicación, atomicidad ni recovery.

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
