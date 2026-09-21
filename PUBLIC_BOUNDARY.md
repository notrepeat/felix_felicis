# Frontera entre `felix_felicis` público e instancias privadas

Este repositorio publica mecanismos, no datos de usuario.

## Puede vivir aquí

- crates del motor, CLI y adaptadores;
- especificaciones y documentación general;
- manifiestos y bundles completamente ficticios;
- fixtures generados y pruebas de propiedades;
- contratos para integrar un bundle externo por ruta.

## Debe vivir en un repositorio privado

- reglas, valores, objetivos, perfiles o recuerdos de una persona;
- transcripciones, citas, evidencias y resúmenes de sesiones;
- identificadores de sesiones o cuentas;
- estados de procesamiento, logs y reportes derivados de uso real;
- manifiestos que revelen la taxonomía de una instancia privada;
- pruebas golden construidas a partir de contenido real.

La integración privada puede depender del motor público. El motor público no
puede depender de la integración privada, ni siquiera en sus pruebas.

## Publicación

El repositorio público se crea con historial nuevo. No se deriva haciendo
público el historial de una instancia, porque borrar archivos del último
commit no elimina sus versiones anteriores.

Antes de publicar se ejecutan las pruebas, el lint y
`python tools/check_public_boundary.py`. El guard es una última red, no un
sustituto de la revisión humana del diff y del historial.
