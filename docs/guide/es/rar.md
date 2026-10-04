---
description: Lee archivos RAR/CBR de solo lectura, con multivolumen, contraseñas y extracción verificada.
group: Reference
order: 20
keywords: rar cbr solo lectura cifrado sólido multivolumen part1 r00
---

# Lector RAR

RAR es un formato **de solo lectura**, activo por defecto en la CLI y la ventana
de escritorio. Las compilaciones normales y los paquetes creados con ellas lo
incluyen:

```sh
cargo build --release
# Solo la CLI:
cargo build --release -p arca-cli
```

El adaptador `arca-rar` fija `rars` en 0.10.0, desactiva sus opciones por defecto
y habilita únicamente `encryption`. No compila su escritor ni necesita UnRAR,
RAR/WinRAR o 7-Zip instalados para funcionar. Para compilar sin RAR, usa
`--no-default-features --features codecs-native`; omite `codecs-native` para
excluir también los códecs nativos de ZIP/7z. Una compilación Rust puro con RAR
usa `--no-default-features --features rar`. No hay un interruptor en la interfaz.

## Uso

```sh
arca list archive.rar
arca list private.rar -p 'password'
arca test archive.rar -p 'password'
arca extract archive.rar -o extracted/ -p 'password'
arca extract comic.cbr -o comic/
arca list backup.part03.rar
arca extract backup.r01 -o backup/
```

Omite `-p` si no hay cifrado. Las contraseñas en la CLI pueden quedar en el
historial o ser visibles en la lista de procesos; en equipos compartidos es
preferible el diálogo de la GUI. La GUI pide la contraseña antes de listar
cabeceras cifradas, permite reintentar contraseñas incorrectas identificables y
la conserva para el archivo abierto. Con cabeceras cifradas RAR4, una contraseña
incorrecta puede producir un error de formato: vuelve a abrir con la contraseña
correcta antes de concluir que el archivo está dañado.

Se admiten listado, comprobación, vistas previas y extracción. Los archivos
sólidos se decodifican secuencialmente. Incluso una extracción parcial verifica
todos los miembros antes de publicar los seleccionados. La comprobación en la
GUI siempre verifica el RAR completo. El título indica que es de solo lectura;
no se añaden asociaciones del sistema operativo.

RAR no aparece entre los formatos de creación. No se permite crear, añadir,
borrar, renombrar, mover, crear carpetas internas ni cambiar contraseñas en RAR.
Copiar el contenedor como archivo no modifica su contenido.

## Conjuntos multivolumen

Mantén todas las partes en la misma carpeta. Puedes abrir cualquier parte de
un conjunto moderno (`name.part1.rar` o `name.part01.rar`) o antiguo
(`name.rar`, `name.r00`, `name.r01`). Arca localiza la primera parte y trabaja
con el conjunto completo, incluidos miembros divididos, sólidos y cifrados.
La contraseña se aplica al conjunto. Para seleccionar un volumen antiguo en
el diálogo de apertura, usa **All files (including RAR volumes)**.

No se buscan partes en otras carpetas ni se descargan volúmenes. Los volúmenes
ausentes, duplicados, desordenados o incoherentes producen un error con la ruta
afectada. Deben ser archivos normales, no enlaces ni puntos de reanálisis de
Windows. Un archivo independiente cuyo nombre parezca multivolumen sigue siendo
independiente si sus cabeceras no declaran un conjunto.
Se ignoran los archivos numerados no seleccionados fuera del rango admitido;
un volumen seleccionado fuera de rango o una parte 257 necesaria siguen fallando.

RAR no tiene un identificador universal de conjunto. Se comprueban los números,
indicadores y metadatos de miembros divididos disponibles. Volúmenes ajenos con
miembros no divididos y metadatos idénticos no siempre se pueden distinguir;
los formatos antiguos tampoco ofrecen siempre números de volumen fiables.
Los conjuntos históricos sin cabecera final pueden continuar después de un
miembro no dividido. Si falta su último volumen en uno de esos límites, los
metadatos disponibles no permiten detectar la ausencia. Si el primer volumen
tiene cabecera final, los siguientes también deben tenerla.

## Seguridad y límites

| Recurso | Límite |
|---|---:|
| Volúmenes por conjunto | 256 |
| Entradas de carpeta inspeccionadas al buscar volúmenes | 100.000 |
| Cabeceras en todo el conjunto | 100.000 |
| Bytes de cabeceras en todo el conjunto | 64 MiB |
| Diccionario RAR5 | 256 MiB |
| Espacio de trabajo del decodificador | 512 MiB |
| Decodificación de filtros RAR5 en memoria | 64 MiB |
| Salida por miembro | 4 GiB |
| Salida total, incluidos miembros no seleccionados | 16 GiB |
| Vista previa en memoria | 64 MiB |

Son límites del decodificador, no un aislamiento estricto de memoria o tiempo
de CPU. Algunos filtros necesitan más memoria de la permitida y se rechazan.
La cancelación se comprueba al buscar, analizar y decodificar, y entre los pasos
de publicación.

Las vistas previas multivolumen limitan a 64 MiB el contenido seleccionado en
memoria; los miembros descartados usan los límites normales de salida. El
backend sigue decodificando y verificando todo el conjunto, por lo que la
corrupción en otro volumen puede impedir la vista previa. En archivos
independientes, la vista previa termina tras el miembro seleccionado.

La extracción decodifica primero en almacenamiento temporal privado y verifica
la integridad. Una contraseña incorrecta, corrupción o fallo de decodificación
deja el destino intacto. Después publica los archivos seleccionados mediante
temporales en el mismo sistema de archivos y reemplazo atómico. Un fallo de E/S
o cancelación **durante la publicación** puede dejar archivos ya verificados:
no se garantiza una transacción sobre todo el árbol de destino. El espacio
temporal necesario puede acercarse al límite de salida.

Se rechazan rutas absolutas, escapes de directorio, nombres de dispositivo,
flujos de Windows, caracteres de control, colisiones de nombres y de
archivo/carpeta. No se admiten enlaces ni archivos especiales RAR. Se rechazan
enlaces y puntos de reanálisis bajo el destino; la raíz elegida se resuelve a
su ubicación canónica. No extraigas en un árbol que otro proceso pueda modificar
simultáneamente: las comprobaciones no aíslan carreras del sistema de archivos.

RAR5 exige una cabecera final completa. Si el formato no guarda CRC32, esa
columna queda vacía, pero se ejecutan sus comprobaciones nativas. La garantía
de integridad depende de las sumas y autenticadores presentes en el archivo.

## Exclusiones y notas sobre dependencias

- No hay escritura RAR, recuperación/reparación ni asociaciones automáticas.
  Los métodos o metadatos no admitidos devuelven un error.
- Los fixtures independientes, comparaciones con UnRAR y fuzzing acotado
  amplían la cobertura, pero no demuestran compatibilidad exhaustiva.
- La [revisión de procedencia](https://github.com/beyondhumane/arca/blob/main/docs/plans/rars-0.10.0-distribution.md)
  recoge la declaración Apache-2.0 del autor y dudas sobre fuentes e
  investigación. No demostró una infracción ni una incompatibilidad de licencia
  que impida distribuir el lector. Esas observaciones no son una garantía legal
  ni un motivo para etiquetarlo como experimental. Los textos de licencia y
  avisos publicados para las dependencias del adaptador acompañan los paquetes.

Comprobaciones reproducibles:

```sh
python3 arca-rar/tests/check-features.py
cargo test --workspace
cargo test --workspace --no-default-features --features codecs-native
cargo clippy --workspace --all-targets -- -D warnings
```

Consulta el [registro de decisiones](https://github.com/beyondhumane/arca/blob/main/docs/todos/rar-format.md)
y la [procedencia de los fixtures](https://github.com/beyondhumane/arca/blob/main/arca-rar/tests/fixtures/README.md).
