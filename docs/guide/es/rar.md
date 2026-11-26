---
description: Lee archivos RAR/CBR, con multivolumen, contraseñas y extracción verificada, y crea nuevos archivos RAR5, sólidos, cifrados, en volúmenes o con registro de recuperación.
group: Reference
order: 20
keywords: rar cbr solo lectura crear rar5 cifrado ocultar nombres sólido multivolumen volúmenes part1 r00 registro de recuperación
---

# RAR

[English](../rar.md) | **Español**

Arca lee todas las familias RAR y **crea nuevos archivos RAR5**: sólidos o no,
cifrados con o sin nombres ocultos, enteros o en volúmenes, con o sin registro
de recuperación. Los archivos RAR y CBR existentes nunca se modifican. RAR está activo
por defecto en la CLI y la ventana de escritorio; las compilaciones normales y
los paquetes creados con ellas lo incluyen:

```sh
cargo build --release
# Solo la CLI:
cargo build --release -p arca-cli
```

El adaptador `arca-rar` fija `rars` en 0.10.0, desactiva sus opciones por defecto
y habilita únicamente `encryption` y `write`; `recovery` y `parallel` siguen
desactivadas. No necesita UnRAR, RAR/WinRAR o 7-Zip instalados para funcionar. Para compilar sin RAR, usa
`--no-default-features --features codecs-native`; omite `codecs-native` para
excluir también los códecs nativos de ZIP/7z. Una compilación Rust puro con RAR
usa `--no-default-features --features rar`. No hay un interruptor en la interfaz.

## Lectura

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
GUI siempre verifica el RAR completo. El título indica que el archivo abierto es
de solo lectura. El instalador de Windows y los paquetes de Linux ofrecen Arca
para `.rar` en Abrir con, junto a los demás tipos de archivo comprimido y solo
si lo eliges; Arca nunca se hace predeterminada. Los cómics `.cbr` no se
ofrecen.

No se permite añadir, borrar, renombrar, mover, crear carpetas internas ni
cambiar contraseñas en un RAR o CBR existente. Copiar el contenedor como archivo
no modifica su contenido.

## Crear archivos RAR5

```sh
arca create backup.rar documents/ notes.txt
arca create photos.rar photos/ -l best
arca create plain.rar big.iso -c store      # equivale a -l store
arca create solido.rar src/ --solid
arca create privado.rar documents/ -p 'contraseña' --hide-names
arca create dvd.rar videos/ -v 4092m --recovery 3%
```

La salida es un archivo RAR5 nuevo que WinRAR, UnRAR y 7-Zip oficiales abren.
`-l store|fast|normal|best` elige la fuerza del compresor RAR; `-c` solo admite
`auto` o `store`. El diálogo **Crear** del escritorio ofrece RAR junto a ZIP y
7z con los mismos niveles, la contraseña y **Ocultar nombres**, una tarjeta
**Opciones de RAR** con **Archivo sólido** y **Registro de recuperación**, y los
tamaños de volumen con que se dividen los demás formatos. Se guardan los
ficheros y directorios vacíos, se conservan las fechas de modificación y los
directorios se recorren en orden. Sin contraseña, la misma entrada produce
siempre los mismos bytes; con ella, las sales son aleatorias.

A partir del nivel 5, incluido el predeterminado `normal`, Arca aplica los
filtros de datos de RAR5 donde compensan, como hace WinRAR desde `-m3`: E8 o
E8E9 en código x86, ARM en código ARM de 32 bits y delta, de 1 a 32 canales,
en audio, imágenes y tablas. Cada tramo de 64 KiB de un fichero se juzga por
separado, así que un ejecutable recibe el filtro de código en su código y
delta en sus tablas. El texto y los datos ya comprimidos se dejan como están.
Los ficheros de más de 64 MiB, que el lector de Arca tendría que descodificar
enteros, y los archivos sólidos se escriben sin filtros. El código ARM64 no
tiene filtro en RAR5.

| Opción | WinRAR | Qué escribe |
| --- | --- | --- |
| `--solid` | `-s` | Un solo flujo de compresión para todos los ficheros, en el orden dado. Más pequeño con muchos ficheros parecidos; un daño alcanza a todos los ficheros posteriores, y extraer uno descodifica los anteriores. |
| `-p CONTRASEÑA` | `-p` | Cada fichero cifrado con AES-256 bajo una clave derivada con PBKDF2-HMAC-SHA256 (2^15 rondas), con un valor de comprobación de la contraseña y sumas de verificación ligadas a ella. Los nombres siguen siendo legibles. |
| `--hide-names` | `-hp` | También las cabeceras cifradas: sin la contraseña ni siquiera se pueden listar los nombres. Exige `-p`. |
| `-v TAMAÑO` | `-v` | Un conjunto de volúmenes `nombre.part1.rar`, `nombre.part2.rar`… (`part01`… a partir de diez), cada uno de como mucho TAMAÑO bytes con todo incluido, de al menos 64 KiB y no más de 256 volúmenes. Un fichero puede quedar cortado entre volúmenes. Un conjunto que cabe en un volumen conserva el nombre indicado, pero sigue siendo un conjunto de un volumen, que `arca repair` rechaza y WinRAR repara. |
| `--recovery PORCENTAJE` | `-rr` | Un registro de recuperación Reed–Solomon de entre el 1 y el 100 %, en el archivo o en cada volumen, que usan `arca repair` y **Reparar** de WinRAR. No se escriben volúmenes de recuperación (`.rev`). |

La creación rechaza, antes de leer ninguna entrada o escribir ningún fichero:

- Una contraseña vacía, **Ocultar nombres** sin contraseña, un registro de
  recuperación fuera del 1 al 100 %, un tamaño de volumen menor de 64 KiB y un
  conjunto de volúmenes sin miembros.
- `-c deflate|zstd|lzma2`, un
  destino `.cbr` o cualquier otro sufijo distinto de `.rar`.
- Un destino que ya exista, incluso como enlace roto, o, para un conjunto de
  volúmenes, cualquier `nombre.partN.rar` del mismo nombre, en cualquier
  mayúscula o número de cifras. Arca solo crea archivos RAR nuevos y nunca
  reemplaza un fichero, tampoco uno que aparezca mientras se escribe. La
  carpeta padre del destino debe existir.
- Entradas que sean enlaces simbólicos, puntos de reanálisis o ficheros
  especiales (no se siguen), entradas que coincidan con el destino o su carpeta,
  y nombres no válidos, duplicados o que solo difieran en mayúsculas.
- Más de 100.000 miembros, nombres de más de 4096 bytes, un fichero de más de
  4 GiB o más de 16 GiB en total.

El escritor prepara un fichero `.arca-*.rar.part` junto al destino, lo reabre
con el propio lector de Arca para listar y decodificar por completo cada
miembro (con la contraseña) y después lo publica sin sobrescribir nada. Un
conjunto de volúmenes se prepara entero en una carpeta privada
`.arca-*.rar.parts` junto al destino, se verifica desde su primer volumen y
sus volúmenes se publican después uno a uno sin sobrescribir nada, el primero
en último lugar; si uno no puede publicarse, los ya publicados se vuelven a
borrar. Los miembros comprimidos y el volumen que se está llenando se vuelcan a
disco, no se guardan en memoria. Cualquier fallo o la cancelación no deja nada
en la ruta de destino. En la CLI, Ctrl+C (SIGINT) o
SIGTERM durante una creación RAR se atiende de forma cooperativa: el recorrido o
el escritor se detienen en su siguiente comprobación, informan `cancelled` con
estado de salida distinto de cero y el fichero preparado se elimina. En Unix se
conserva el tratamiento heredado de SIGHUP para respetar `nohup`. Un SIGHUP no
ignorado, SIGKILL o un corte de corriente pueden dejar temporales
`.arca-*.rar.part` y `.rars-spool-*`, que no son archivos publicados.

No se admite, y se rechaza en lugar de aproximarse: volúmenes de recuperación
(`.rev`), comentarios de archivo y de fichero, salida RAR4, creación de CBR, y añadir, borrar o renombrar dentro de un archivo
existente. Un archivo dañado de un solo volumen que tenga registro de
recuperación se reconstruye en un archivo nuevo con
[`arca repair`](cli-reference.md#repair); un volumen dañado de un conjunto lo
repara WinRAR (`Rar r` sobre ese volumen) con el registro de recuperación que
lleva. Arca aún no abre un conjunto cuyo volumen haya reconstruido WinRAR sin
cabeceras cifradas: el bloque final reconstruido ya no dice que sigue otro
volumen, y el lector de Arca lo exige a un volumen que corta un fichero. El comentario del
archivo lo muestran `arca list`, `arca comment` y la barra de estado de la
ventana.

Cada opción se comprobó con el `Rar.exe` de consola de WinRAR 7.23: `Rar t` y
`Rar x` aceptan lo que Arca escribe con cada opción y con todas juntas, entero
y en volúmenes, y `Rar r` reconstruye con el registro de recuperación de Arca un
archivo dañado en sus datos. A su vez, Arca lee los archivos sólidos, `-p`,
`-hp`, `-v` y `-rr` de WinRAR. Las comprobaciones son pruebas ignoradas que
necesitan `Rar.exe`:

```sh
cargo test -p arca-rar --features rar --test writer_features -- --ignored
```

`ARCA_RAR` indica otro `Rar.exe`; por defecto es
`C:\Program Files\WinRAR\Rar.exe`.

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

La creación tiene sus propios techos (miembros, bytes de nombre, 4 GiB por
fichero, 16 GiB en total, 64 MiB de cabeceras y un libro de memoria gestionada
de 256 MiB para el escritor; consulta
[`arca_rar::create_limits`](https://github.com/beyondhumane/arca/blob/main/crates/arca-rar/src/lib.rs)).
Son límites del decodificador y del escritor, no un aislamiento estricto de
memoria o tiempo de CPU. Algunos filtros necesitan más memoria de la permitida y se rechazan.
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

- No se escriben comentarios ni volúmenes de recuperación, ni hay modificación de archivos existentes ni
  asociaciones automáticas. Los métodos o metadatos no admitidos devuelven un
  error. La creación se limita al perfil RAR5 descrito arriba.
- Los fixtures independientes, comparaciones con UnRAR y fuzzing acotado
  amplían la cobertura del lector; el harness de creación con semilla compara
  los archivos que Arca escribe con UnRAR y 7-Zip oficiales. No demuestran
  compatibilidad exhaustiva. Consulta el
  [registro de validación del lector](https://github.com/beyondhumane/arca/blob/main/crates/arca-rar/RAR_VALIDATION.md)
  y el [registro de validación de la creación](https://github.com/beyondhumane/arca/blob/main/crates/arca-rar/RAR_CREATE_VALIDATION.md).
- La [revisión de procedencia](https://github.com/beyondhumane/arca/blob/main/docs/plans/rars-0.10.0-distribution.md)
  recoge la declaración Apache-2.0 del autor y dudas sobre fuentes e
  investigación. No demostró una infracción ni una incompatibilidad de licencia
  que impida distribuir el lector. Esas observaciones no son una garantía legal
  ni un motivo para etiquetarlo como experimental. Los textos de licencia y
  avisos publicados para las dependencias del adaptador acompañan los paquetes.
  Activar el escritor no añadió ninguna dependencia que la rama del lector no
  tuviera ya, así que el paquete de avisos no cambia.

Comprobaciones reproducibles:

```sh
python3 crates/arca-rar/tests/check-features.py
cargo test --workspace
cargo test --workspace --no-default-features --features codecs-native
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p arca-cli
python3 crates/arca-rar/tests/create-stress.py            # smoke acotado; omite las herramientas ausentes
UNRAR=/ruta/a/unrar SEVENZIP=/ruta/a/7zz python3 crates/arca-rar/tests/create-stress.py --stress --require-tools
```

Consulta el [registro de decisiones](https://github.com/beyondhumane/arca/blob/main/docs/todos/rar-format.md)
y la [procedencia de los fixtures](https://github.com/beyondhumane/arca/blob/main/crates/arca-rar/tests/fixtures/README.md).
