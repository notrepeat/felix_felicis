# 08 — Validar contra uso real sin publicar datos personales

El objetivo es usar la instancia privada como prueba ecológica del motor
público, conservando una frontera física y verificable entre producto y datos.

## Estado

| Campo | Valor |
|---|---|
| Estado | completo (2026-11-18); **legacy** desde el 2026-11-18: la instancia privada queda fijada en `v0.2.1` y no sigue los tags nuevos. La prueba de uso real pasa al repositorio de criterio de la [fase 10](10-criterio-distribuible.md) (U10). |
| Depende de | 04 |
| Desbloquea | 09 |

## Entregables

1. El repositorio privado consume un tag público explícito.
2. Suite de integración privada sobre el bundle real.
3. Guard que impide copiar fixtures reales al repositorio público.
4. Informe agregado sin títulos, citas, paths ni identificadores de sesión.
5. Procedimiento para reportar un bug mediante una reproducción ficticia.

## Revisión de licencia

La separación entre repositorios y datos no determina por sí sola las
obligaciones de licencia del código de integración. Antes de enlazar o
combinar el motor AGPL con el adaptador privado, o de exponer una versión
modificada por red, documenta una revisión del modelo concreto de uso y
distribución. No supongas que el repositorio de integración puede permanecer
privado solo porque el bundle y sus fixtures no se publican.

## Criterios de aceptación

- [x] El motor público no conoce la ubicación del repositorio privado.
- [x] La integración privada puede actualizar de una versión etiquetada a otra: pasó del commit `88bd5fe` al tag `v0.2.0` con el procedimiento escrito, y la suite volvió a dar ok.
- [x] Los fallos reales se reducen a fixtures sintéticos antes de publicarse (procedimiento escrito en la integración privada; aún no hubo fallo que publicar).
- [x] El escáner de privacidad sigue verde después de cada reproducción. Hay además un guardia privado→público que busca slugs y títulos de la biblioteca en los archivos versionados de este repositorio: 0 coincidencias.
- [x] Ningún log de CI público contiene contenido del bundle privado (el CI público no monta la biblioteca).
- [x] Las métricas agregadas no permiten reconstruir conceptos personales: el informe privado sólo tiene conteos, veredictos, ids de regla, códigos y tiempos.

## Fuera de alcance

- Publicar el historial o los conceptos privados.
- Ejecutar CI público con secretos que monten el bundle real.
- Tratar la instancia privada como dependencia del motor.

## Verificación prevista

```powershell
# Se ejecuta únicamente en el repositorio privado.
cargo test --test private_bundle_integration
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-11-18 | motor `88bd5fe` | Integración privada: `validate`, `check` con cinco reglas de la especificación de conducta y `propose` sobre copia temporal, en una biblioteca de 93 conceptos y 104 relaciones. Revisión de licencia: la integración invoca la CLI como proceso aparte, sin enlazar el motor. | `validate` y `check` aceptan la biblioteca; `propose` rechaza un `standing` inválido con la regla correcta y aplica y sella una promoción, con `guard` en `pass`. Hallazgos: el manifiesto histórico declaraba un directorio que ya no existe; una regla sobre una arista y dos nodos (~900 000 combinaciones) excede el presupuesto de evaluación; `check --explain` excede el suyo aun sin fallos. |
| 2026-11-18 | `v0.2.0` (`e2ec274`) | La integración privada fija el tag y repite la suite. | Resultado `ok`: `check` admite, y `propose` admite, rechaza y admite en los tres casos sintéticos; el guardia privado→público da 0 coincidencias. |
| 2026-11-18 | `v0.2.1` (`8a6dd25`) | La integración pasa al tag nuevo y suma cuatro reglas sobre aristas, escritas con guarda. | Resultado `ok`: `check` admite las nueve reglas; la forma sin guarda sigue excediendo el presupuesto y queda registrada como límite. |

## Siguiente fase

[09 — Versionado y evidencia](09-versionado-y-evidencia.md).
