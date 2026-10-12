# Changelog

Los cambios se agrupan por release. La sección `Unreleased` es una propuesta
para el próximo gate `0.2.0`; aún no hay tag ni actualización de versiones.

## Unreleased — candidato a 0.2.0

### Límites y seguridad de carga

- `cargar_bundle` limita la lectura desde disco a 128 MiB de contenido Markdown,
  100.000 entradas, profundidad 128 y 16 MiB de rutas lógicas acumuladas. Las
  cuotas son inclusivas y globales a todas las raíces declaradas.
- Si una entrada o symlink no Markdown no se puede clasificar o recorrer de
  forma segura, la carga falla en vez de omitirla y devolver un bundle parcial.
- `parsear_bundle` recibe documentos ya materializados y queda fuera de las
  cuotas de `cargar_bundle`.

### Compatibilidad pre-1.0

- Se agrega `Regla::LimiteBundleExcedido` y se expone
  `ErrorEstructura::HijoDuplicadoEnLote` para diagnosticar asignaciones `Parent`
  duplicadas en una operación por lote. Los consumidores con `match`
  exhaustivos deben contemplar ambas variantes.
- `layout.directorios` exige rutas disjuntas: no se admiten declaraciones
  iguales ni rutas que sean ancestro/descendiente entre sí.
- Para errores, `regla` y `path` sirven para clasificar y localizar. El campo
  `detalle` es texto humano no estable y no debe analizarse programáticamente.

Las versiones de los crates permanecen en `0.1.0` hasta cerrar el gate de
release y publicar la migración a `0.2.0`.
