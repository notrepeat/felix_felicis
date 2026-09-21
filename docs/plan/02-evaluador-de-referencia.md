# 02 — Ejecutar la semántica completa del AST

El objetivo es disponer de un evaluador deliberadamente simple y correcto,
basado en la semántica de Tarski. Será el oráculo contra el que se comparará
cualquier optimización futura.

## Estado

| Campo | Valor |
|---|---|
| Estado | pendiente |
| Depende de | 01 |
| Desbloquea | 03 |

## Punto de partida

- [x] Existe un AST FO bisortido.
- [x] Existe parser de S-expresiones con errores de autoría.
- [x] El grafo expone nodos, aristas, padres, etiquetas y atributos.
- [ ] Existe evaluación de fórmulas sobre una estructura.

## Entregables

1. Entorno de asignación separado para variables de nodo y arista.
2. Evaluación de predicados base, extensiones e igualdad.
3. Evaluación de conectivos, cuantificadores, guardas y `TC[Parent]`.
4. Instanciación finita de esquemas.
5. Modos `decidir` y `enumerar` con cortocircuito explícito.
6. Reporte de variables totales, anchura viva y tamaño de estructura.

## Criterios de aceptación

- [ ] Cada variante de `Formula` tiene al menos un caso verdadero y uno falso.
- [ ] Las sortes incorrectas se rechazan antes de evaluar.
- [ ] Los esquemas producen el conjunto determinista esperado de oraciones.
- [ ] `TC[Parent]` coincide con recorridos explícitos en bosques generados.
- [ ] Pruebas generativas comparan el evaluador con un oráculo independiente.
- [ ] No se introduce una optimización sin conservar el evaluador de referencia.

## Fuera de alcance

- Extracción de testigos mínimos.
- Check incremental.
- Promesas de latencia.

## Verificación prevista

```powershell
cargo test -p universo-core evaluador
cargo test -p universo-core --test evaluador_prop
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| — | — | — | — |

## Siguiente fase

[03 — Veredictos explicables](03-veredictos-explicables.md).
