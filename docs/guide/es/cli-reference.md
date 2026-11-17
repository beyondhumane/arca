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
  bench     Measure the R1 and R2 performance requirements

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

Mide los requisitos de rendimiento R1 y R2 contra un archivo.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo contra el que medir. |

## Estado de salida

| Estado | Significado |
| --- | --- |
| `0` | Éxito. |
| `1` | Error de archivo/E/S, incluida contraseña incorrecta o datos cifrados corruptos. Se imprime como `arca: <message>` en la salida de error. También un `hash` que no pudo leer un fichero o encontró una suma que no coincide. |
| `2` | Sintaxis u opciones CLI inválidas, indicadas por el analizador de argumentos. |
</content>
