---
description: Cada comando, argumento, opción y valor por defecto del binario arca.
group: Referencia
order: 11
keywords: cli línea de comandos opciones flags referencia ayuda estado de salida command line options flags reference help exit status
---

# Referencia de la CLI

ZIP, 7z, TAR (sin comprimir, gzip, XZ, bzip2 y Zstandard), XZ, bzip2 y Zstandard sueltos y [RAR](rar.md) están activos por defecto. RAR permite `list`,
`test` y `extract`, con `-p` para datos o cabeceras cifradas, y `create` para
nuevos archivos RAR5 de un solo volumen. Modificar un RAR o CBR existente, y
crear CBR, siempre se rechaza.

```text
Fast, safe archiver

Usage: arca <COMMAND>

Commands:
  create    Create an archive                                          [aliases: c]
  list      List the contents without extracting them                  [aliases: l]
  extract   Extract the contents                                       [aliases: x]
  test      Check integrity without writing to disk                    [aliases: t]
  hash      Print or check checksums of files, in the sha256sum format
  password  Add, change or remove the password of an existing archive
  repair    Rebuild a damaged ZIP from the entries that can still be read
  diff      Compare two archives, or an archive and a folder
  bench     Measure every codec on generated data (--codecs), or the R1 and R2 requirements on an archive

Options:
  -h, --help     Print help
  -V, --version  Print version
```

`list`, `extract` y `test` también leen los [contenedores ZIP](zip-containers.md) (.apk, .jar, .epub, .cbz y el resto), las [imágenes ISO 9660](iso.md) y, en compilaciones con [RAR](rar.md), .rar y .cbr. Son de solo lectura: `password` los rechaza y `create` los rechaza todos salvo los archivos `.rar` nuevos. Cualquier parte de un [conjunto dividido](creating-archives.md#dividir-en-partes), como `copia.zip.002`, abre el conjunto entero; los conjuntos divididos también son de solo lectura.

## create · c

Crea un archivo. El formato se toma de la extensión del archivo de salida.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<OUT>` | obligatorio | Archivo de salida: .zip, .7z, .rar, .tar, .tar.gz (o .tgz), .tar.xz (o .txz), .xz, .tar.bz2 (o .tbz2, .tbz), .bz2, .tar.zst (o .tzst), .zst. Un .xz, .bz2 o .zst admite un único fichero. |
| `<INPUTS>...` | obligatorio | Archivos o directorios a incluir. |
| `-l, --level <LEVEL>` | normal | store · fast · normal · best. |
| `-c, --codec <CODEC>` | auto | auto · store · deflate · zstd · lzma2. auto usa Deflate en .zip, LZMA2 en .7z, .xz y .tar.xz, bzip2 en .bz2 y .tar.bz2, Zstandard en .zst y .tar.zst y el compresor RAR en .rar (solo auto o store). |
| `-j, --threads <N>` | 0 | Hilos ZIP y XZ; 0 usa todos los núcleos. XZ usa tantos como quepan en 2 GiB, Zstandard tantos como se pidan y bzip2 uno; 7z y RAR usan todos los núcleos que permite su presupuesto de memoria, diga lo que diga esta opción. |
| `-p, --password <PASSWORD>` | ninguna | Cifra con AES-256 (.zip, .7z). Otras herramientas la pedirán para abrir el archivo. Se rechaza en .rar. |
| `--hide-names` | desactivado | Solo 7z: cifra cabeceras; exige una contraseña no vacía. |
| `-v, --volume <SIZE>` | ninguno | Divide en partes de SIZE bytes, o con k, m o g para KiB, MiB, GiB: `salida.zip.001`, `salida.zip.002`… como hace 7-Zip. Se niega a sustituir un `.001` existente. No en .rar. Véase [dividir en partes](creating-archives.md#dividir-en-partes). |
| `--sfx` | desactivado | Escribe un programa de Windows que se extrae solo (cualquier nombre, normalmente `.exe`) a partir del `arca-sfx.exe` que hay junto a arca. No con `-p`, `--hide-names` ni `-v`. Véase [programas que se extraen solos](creating-archives.md#programas-que-se-extraen-solos). |
| `--console` | desactivado | Con `--sfx`: el programa pide la carpeta e informa en la consola en lugar de en ventanas. |

## list · l

Lista el contenido sin extraerlo.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo a leer. |
| `-t, --time` | desactivado | Informa cuánto tardó (por la salida de error estándar). |
| `-p, --password <PASSWORD>` | ninguna | Contraseña para cabeceras 7z cifradas; no hace falta si los nombres son visibles. |

En un `.zip`, el comentario del archivo y los de cada entrada salen por la salida de error estándar después de la lista, así que la salida estándar sigue siendo una línea por entrada. Los caracteres de control se muestran como `�` para que un comentario no pueda manejar la terminal.

## extract · x

Extrae el contenido.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo a extraer. |
| `-o, --dest <DEST>` | . | Directorio de destino. |
| `--on-conflict <POLICY>` | overwrite | overwrite · skip · rename. Qué hacer cuando el archivo ya existe. |
| `-j, --threads <N>` | 0 | Hilos a usar. 0 significa todos los núcleos. Solo .zip puede ir en paralelo. |
| `-p, --password <PASSWORD>` | ninguna | Contraseña de un archivo cifrado (AES-256 o ZipCrypto). |

## test · t

Comprueba la integridad sin escribir en disco.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo a comprobar. |
| `-p, --password <PASSWORD>` | ninguna | Contraseña de un archivo cifrado (AES-256 o ZipCrypto). |

## hash

Imprime una suma de comprobación por fichero, una línea cada una en el formato que escribe `sha256sum` (`<hex>  <ruta>`), o verifica un fichero con esas líneas. Las carpetas se recorren enteras, por orden de nombre. Los ficheros se leen por partes, nunca enteros en memoria, y se calculan varios a la vez repartidos entre los núcleos; aun así, las líneas salen en orden.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<PATHS>...` | obligatorio salvo con `--check` | Ficheros o carpetas de los que calcular la suma. |
| `-a, --algorithm <ALGORITHM>` | sha256 | crc32 · sha1 · sha256. Con `--check`, el algoritmo de cada línea se deduce de la longitud de su suma, salvo que se indique este. |
| `-c, --check <SUMS>` | ninguno | Verifica los ficheros listados en `SUMS` e imprime `<ruta>: OK` o `<ruta>: FAILED` para cada uno. |

```sh
arca hash fotos > SHA256SUMS
arca hash --check SHA256SUMS     # o: sha256sum -c SHA256SUMS
arca hash -a crc32 setup.exe
```

Las rutas se imprimen tal como se dieron, con `/` entre carpetas en todos los sistemas. Un fichero de sumas puede llevar `*` delante del nombre (modo binario), finales de línea de Windows, una marca de orden de bytes y los nombres escapados que escribe `sha256sum` para nombres con una barra invertida o un salto de línea. Las líneas que no son sumas se avisan y se saltan. Un fichero que no se puede leer, una suma que no coincide o un fichero de sumas sin ninguna línea válida dan el estado de salida 1. BLAKE3 no se ofrece: su crate de Rust necesita por defecto un compilador de C.

## diff

Compara dos archivos, o un archivo y una carpeta, e imprime una línea por cada nombre en el que los dos lados difieren. Cada lado puede ser cualquier archivo que lea `list`, un juego de volúmenes RAR nombrado por cualquiera de sus volúmenes, o una carpeta. Las entradas se leen directamente de los archivos y nunca se extraen.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<A>` | obligatorio | Primer archivo o carpeta. |
| `<B>` | obligatorio | Segundo archivo o carpeta. |
| `--quick` | no | Compara tamaños, y el CRC32 cuando los dos lados lo guardan, sin leer ningún contenido. |
| `--times` | no | Informa también de los ficheros con el mismo contenido cuyas fechas de modificación se separan más de 2 segundos. |
| `-p, --password <PASSWORD>` | ninguna | Contraseña del primer archivo, y del segundo salvo que se dé `--password-b`. |
| `--password-b <PASSWORD>` | la de `-p` | Contraseña del segundo archivo. |

```sh
arca diff v1.0.zip v1.1.zip
arca diff copia.tar.zst ~/proyecto       # ¿la copia sigue coincidiendo con la carpeta?
arca diff --quick -p secreto a.7z b.zip
```

| Línea | Significado |
| --- | --- |
| `+ ruta` | Solo en B. |
| `- ruta` | Solo en A. |
| `M ruta` | En los dos, con distinto contenido, o un enlace que apunta a otro sitio. |
| `T ruta` | Un fichero en un lado y una carpeta o un enlace en el otro. |
| `m ruta` | Mismo contenido y distinta fecha de modificación (solo con `--times`). |

- Las líneas salen por orden de nombre. Los nombres usan `/` y las carpetas terminan en `/`. Se comparan tal como están escritos, así que las mayúsculas cuentan. Se ignoran un `./` inicial y las barras dobles, y una barra invertida se lee como `/`.
- Una carpeta se compara por lo que contiene, no por su propio nombre: `arca diff a.zip salida` después de `arca x a.zip -o salida` no imprime nada. Las carpetas que solo aparecen en la ruta de sus ficheros cuentan como carpetas, así que un archivo sin entradas de carpeta sigue coincidiendo con lo que se extrae de él.
- Por defecto, los ficheros del mismo tamaño se leen en los dos lados y se comparan por SHA-256, salvo que los dos lados guarden CRC32 y no coincidan. Un `.xz`, `.bz2` o `.zst` suelto no tiene un tamaño fiable, así que siempre se lee, salvo con `--quick`, que nunca lo da por cambiado.
- Los enlaces solo se distinguen en TAR y en el disco. Un enlace duro de TAR cuenta como el fichero que nombra. Los dispositivos y las tuberías se dejan fuera.
- Un lado que no se puede abrir, una contraseña incorrecta o ausente, o una entrada que hay que leer y no se puede, dan el estado de salida 2 en vez de una diferencia.

## password

Reescribe un `.zip` con otra contraseña, o sin ninguna. Las entradas no se vuelven a comprimir, porque el cifrado WinZip AES cifra los bytes ya comprimidos.

No se admite cambiar contraseñas ni modificar archivos 7z existentes. Consulta las restricciones de códecs y el cifrado de entradas vacías en [Crear archivos](creating-archives.md#7z).

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo a reescribir (.zip). |
| `-p, --password <PASSWORD>` | ninguna | Contraseña actual, si el archivo tiene una. |
| `--new <NEW>` | ninguna | Contraseña nueva. Omítela para quitar el cifrado. |
| `-o, --out <PATH>` | en el mismo sitio | Escribe aquí en vez de reemplazar el archivo en el mismo sitio. |

## repair

Reconstruye un ZIP cuyo directorio central falta o está dañado. Arca recorre el archivo en busca de cabeceras locales, toma los tamaños de cada entrada de su cabecera o, en las entradas escritas a chorro, del descriptor de datos que sigue a los datos, la descomprime y compara su CRC, y copia byte a byte las entradas que pasan la comprobación a un archivo nuevo. Las entradas descartadas se listan por la salida de error estándar con su desplazamiento y el motivo.

```sh
arca repair roto.zip -o reparado.zip
```

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo dañado. Solo se lee; nunca se modifica. |
| `-o, --out <PATH>` | obligatorio | Dónde escribir el archivo reconstruido (.zip). No puede ser el propio archivo dañado. |

- Las entradas cifradas no se pueden comprobar sin la contraseña: se copian tras una comprobación solo estructural y se nombran por la salida de error estándar.
- Se pierde lo que solo guardaba el directorio central: los comentarios del archivo y de las entradas, los permisos Unix y otros atributos externos.
- Un ZIP guardado dentro de una entrada se queda dentro de ella. Pero si esa entrada exterior está dañada, las entradas interiores pueden aparecer como entradas propias.
- El archivo reconstruido se escribe con un nombre temporal, se vuelve a leer y solo entonces se renombra. Si no se puede recuperar nada, no se escribe nada y la orden falla.

## bench

Dos pruebas. Con `--codecs` se mide cada códec con el que Arca escribe sobre datos generados en memoria, con un hilo y con `--threads`; el método y la puntuación están en [Mide tu propia máquina](benchmarks.md#mide-tu-propia-máquina). Con un archivo en su lugar, se miden contra él los requisitos de rendimiento R1 y R2.

```sh
arca bench --codecs
arca bench --codecs --threads 4 --seconds 2 --json
arca bench big.zip
```

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio sin `--codecs` | Archivo contra el que medir R1 y R2. No va con `--codecs`. |
| `--codecs` | desactivado | Mide la velocidad de compresión y de descompresión y la relación de Store, Deflate, Zstandard, LZMA2, bzip2 y RAR5, e imprime una puntuación. No se lee ni se escribe nada en el disco. |
| `-j, --threads <N>` | `0` | Hilos de la segunda medición; `0` significa todos los núcleos. Con `1` solo se mide un hilo. |
| `--seconds <S>` | `1` | Segundos que cada medición repite sus pasadas, de 0 a 3600. Siempre se hace al menos una. |
| `--size <MIB>` | `16` | MiB de datos generados, de 1 a 4096; se suben a 1 MiB por hilo. |
| `--json` | desactivado | Imprime un objeto JSON en lugar de la tabla: velocidades en MB/s, relaciones como tamaño comprimido entre original y `null` para lo que no se midió. |

La tabla y el JSON terminan con la orden que los reproduce. El progreso va a la salida de error estándar, y solo cuando es un terminal. Ctrl+C detiene la prueba entre pasadas, imprime lo medido y sale con el estado 130.

## Estado de salida

| Estado | Significado |
| --- | --- |
| `0` | Éxito. |
| `1` | Error de archivo/E/S, incluida contraseña incorrecta o datos cifrados corruptos. Se imprime como `arca: <message>` en la salida de error. También un `hash` que no pudo leer un fichero o encontró una suma que no coincide, y un `diff` que encontró diferencias. |
| `2` | Sintaxis u opciones CLI inválidas, indicadas por el analizador de argumentos. También un `diff` que no pudo leer uno de sus lados: en `diff`, 1 solo significa que difieren. |
| `130` | `bench --codecs` detenido con Ctrl+C, después de imprimir lo medido. |
</content>
