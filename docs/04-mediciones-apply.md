# Medición de apply transaccional

La fase 04 conserva la validación completa y el replay. No introduce una ruta
incremental: esta medición caracteriza el caller completo y no identifica qué
paso consume el tiempo. Cualquier optimización posterior debe perfilar esos
pasos y conservar los mismos controles de identidad, CAS, replay y recuperación.

## Reproducción

```sh
cargo test --locked --release -p universo-okf --test apply_publico runtime_apply_dos_writers_1_y_10_mib -- --ignored --nocapture
```

El harness crea un bundle temporal por muestra, con un nodo Markdown, un archivo
auxiliar de exactamente 1 o 10 MiB y directorios auxiliares vacíos. Dos procesos
independientes reciben el mismo token y esperan una señal común. Cada uno llama
`aplicar_archivo` una sola vez. Se exige exactamente un `Aplicada` y un
`writer_lock_busy` o `stale`; también se verifican el contenido nuevo y todos los
bytes del archivo auxiliar. El bundle temporal se elimina al terminar la muestra.

La medida `wall_us` empieza al emitir la señal y termina después de esperar la
salida de ambos hijos. Excluye compilación, creación del fixture, simulación que
obtiene el token y arranque de los hijos. Incluye planificación de procesos,
lectura de configuración, adquisición del lock, CAS, propuesta, staging,
publicación, replay y cleanup del caller. No es una medida exacta de tiempo con
el lock retenido ni un perfil por fase. Los hijos también imprimen, mediante sus
reportes al padre, la duración de cada llamada.

## Resultados locales

Ejecución del 17 de noviembre de 2026 con Rust 1.88.0, perfil `release`, Linux
`7.2.15-1-omarchy`, x86_64, Intel Core i9-11900H. Los fixtures de `tempfile` estaban
en `/tmp`, filesystem **tmpfs**: estos números no miden almacenamiento físico,
`fsync`, durabilidad ante pérdida de energía ni comportamiento de Windows.
Cinco muestras secuenciales por tamaño, sin aislamiento del resto de la máquina.

| Payload auxiliar | Muestras | Mínimo (ms) | Mediana (ms) | Máximo (ms) | Resultados |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 MiB | 5 | 501,729 | 506,130 | 509,909 | 5 applied, 5 busy |
| 10 MiB | 5 | 5.320,440 | 5.430,125 | 5.679,657 | 5 applied, 5 busy |

Valores `wall_us` sin redondear:

```text
1 MiB: 506720, 509909, 501729, 506130, 502837
10 MiB: 5344520, 5320440, 5455771, 5430125, 5679657
```

Las diez muestras completaron las comprobaciones de contenido y exclusión de
writers. En estas muestras el segundo proceso obtuvo `busy`; la prueba normal
del caller acredita además `stale` después de una aplicación y antes del parseo
de un bundle o configuración que cambiaron a bytes inválidos.

La latencia mediana del caso de 10 MiB es aproximadamente 10,73 veces la del caso
de 1 MiB. Es una razón entre dos fixtures, no una ley de escalabilidad. La muestra
es suficiente para decidir **no introducir ahora una optimización incremental
sin perfil ni objetivo de latencia**: hacerlo añadiría otra ruta de validación
sin evidencia de cuál es el coste dominante. Queda registrada una línea base
reproducible para comparar cambios futuros; no define un SLA ni acredita cuotas
de RSS. Antes de una optimización se requieren perfiles por etapa, otros tamaños,
números de archivos y almacenamiento físico, y repetir los tests adversariales y
de cortes de proceso de la ruta completa.
