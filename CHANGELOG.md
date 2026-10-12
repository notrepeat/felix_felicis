# Changelog

Los cambios se agrupan por release. La sección `Unreleased` es una propuesta
para el próximo gate `0.2.0`; aún no hay tag ni actualización de versiones.

## Unreleased — candidato a 0.2.0

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
  SegmentoTerminaEnEspacio, DispositivoReservado, CaracterNoPortable}`;
  `ErrorManifiesto::{DirectoriosDuplicados, DirectoriosSolapados,
  DirectoriosRutaInvalida, SinIotaRutaInvalida, IgnoradosRutaInvalida}`;
  `ErrorEstructura::HijoDuplicadoEnLote`; `ErrorSnapshot::LimiteInventarioExcedido`
  y `Regla::LimiteBundleExcedido`. Los consumidores con `match` exhaustivos
  deben contemplar estas variantes.
- `BundlePath` y las rutas de `layout.directorios`/`layout.sin_iota` exigen
  segmentos portables a Windows; `layout.ignorados` exige un nombre de un solo
  segmento portable. Se rechazan caracteres Windows no admitidos, puntos o
  espacios finales y dispositivos DOS reservados.
- Renombra los archivos y directorios que incumplan esas reglas y actualiza
  `layout.directorios`, `layout.sin_iota`, `layout.ignorados`, los wikilinks y
  cualquier referencia `Parent` explícita que apunte a las rutas renombradas.
- `layout.directorios` exige rutas disjuntas: no se admiten declaraciones
  iguales ni rutas que sean ancestro/descendiente entre sí; también se rechazan
  raíces que resuelvan por symlink/junction al mismo directorio o a directorios
  físicamente solapados.
- Para errores, `regla` y `path` sirven para clasificar y localizar. El campo
  `detalle` es texto humano no estable y no debe analizarse programáticamente;
  si se exceden varias cuotas a la vez, cuál se reporta primero tampoco es un
  contrato estable.

Las versiones de los crates permanecen en `0.1.0` hasta cerrar el gate de
release y publicar la migración a `0.2.0`.
