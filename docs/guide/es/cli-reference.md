---
description: Cada comando, argumento, opción y valor por defecto del binario arca.
group: Referencia
order: 11
keywords: cli línea de comandos opciones flags referencia ayuda estado de salida command line options flags reference help exit status
---

# Referencia de la CLI

ZIP, 7z, TAR y [RAR](rar.md) están activos por defecto. RAR permite `list`,
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
  password  Add, change or remove the password of an existing archive
  bench     Measure the R1 and R2 performance requirements

Options:
  -h, --help     Print help
  -V, --version  Print version
```

`list`, `extract` y `test` también leen los [contenedores ZIP](zip-containers.md) (.apk, .jar, .epub, .cbz y el resto), las [imágenes ISO 9660](iso.md) y, en compilaciones con [RAR](rar.md), .rar y .cbr. Son de solo lectura: `password` los rechaza y `create` los rechaza todos salvo los archivos `.rar` nuevos.

## create · c

Crea un archivo. El formato se toma de la extensión del archivo de salida.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<OUT>` | obligatorio | Archivo de salida: .zip, .7z, .rar, .tar, .tar.gz (o .tgz). |
| `<INPUTS>...` | obligatorio | Archivos o directorios a incluir. |
| `-l, --level <LEVEL>` | normal | store · fast · normal · best. |
| `-c, --codec <CODEC>` | auto | auto · store · deflate · zstd · lzma2. auto usa Deflate en .zip, LZMA2 en .7z y el compresor RAR en .rar (solo auto o store). |
| `-j, --threads <N>` | 0 | Hilos ZIP; 0 usa todos los núcleos. La creación 7z y RAR es secuencial; .rar rechaza N mayor que 1. |
| `-p, --password <PASSWORD>` | ninguna | Cifra con AES-256 (.zip, .7z). Otras herramientas la pedirán para abrir el archivo. Se rechaza en .rar. |
| `--hide-names` | desactivado | Solo 7z: cifra cabeceras; exige una contraseña no vacía. |

## list · l

Lista el contenido sin extraerlo.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo a leer. |
| `-t, --time` | desactivado | Informa cuánto tardó (por la salida de error estándar). |
| `-p, --password <PASSWORD>` | ninguna | Contraseña para cabeceras 7z cifradas; no hace falta si los nombres son visibles. |

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

## password

Reescribe un `.zip` con otra contraseña, o sin ninguna. Las entradas no se vuelven a comprimir, porque el cifrado WinZip AES cifra los bytes ya comprimidos.

No se admite cambiar contraseñas ni modificar archivos 7z existentes. Consulta las restricciones de códecs y el cifrado de entradas vacías en [Crear archivos](creating-archives.md#7z).

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo a reescribir (.zip). |
| `-p, --password <PASSWORD>` | ninguna | Contraseña actual, si el archivo tiene una. |
| `--new <NEW>` | ninguna | Contraseña nueva. Omítela para quitar el cifrado. |
| `-o, --out <PATH>` | en el mismo sitio | Escribe aquí en vez de reemplazar el archivo en el mismo sitio. |

## bench

Mide los requisitos de rendimiento R1 y R2 contra un archivo.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo contra el que medir. |

## Estado de salida

| Estado | Significado |
| --- | --- |
| `0` | Éxito. |
| `1` | Error de archivo/E/S, incluida contraseña incorrecta o datos cifrados corruptos. Se imprime como `arca: <message>` en la salida de error. |
| `2` | Sintaxis u opciones CLI inválidas, indicadas por el analizador de argumentos. |
</content>
