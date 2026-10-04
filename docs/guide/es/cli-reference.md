---
description: Cada comando, argumento, opción y valor por defecto del binario arca.
group: Referencia
order: 11
keywords: cli línea de comandos opciones flags referencia ayuda estado de salida command line options flags reference help exit status
---

# Referencia de la CLI

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

`list`, `extract` y `test` también leen los [contenedores ZIP](zip-containers.md) (.apk, .jar, .epub, .cbz y el resto) y, en compilaciones con el [lector de RAR](rar.md), .rar y .cbr. Son de solo lectura: `create` y `password` los rechazan.

## create · c

Crea un archivo. El formato se toma de la extensión del archivo de salida.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<OUT>` | obligatorio | Archivo de salida: .zip, .tar, .tar.gz (o .tgz). |
| `<INPUTS>...` | obligatorio | Archivos o directorios a incluir. |
| `-l, --level <LEVEL>` | normal | store · fast · normal · best. |
| `-c, --codec <CODEC>` | auto | auto · store · deflate · zstd. auto usa Deflate en .zip por compatibilidad. |
| `-j, --threads <N>` | 0 | Hilos a usar. 0 significa todos los núcleos. |
| `-p, --password <PASSWORD>` | — | Cifra con AES-256. Otras herramientas la pedirán para abrir el archivo. |

## list · l

Lista el contenido sin extraerlo.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo a leer. |
| `-t, --time` | desactivado | Informa cuánto tardó (por la salida de error estándar). |

## extract · x

Extrae el contenido.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo a extraer. |
| `-o, --dest <DEST>` | . | Directorio de destino. |
| `--on-conflict <POLICY>` | overwrite | overwrite · skip · rename. Qué hacer cuando el archivo ya existe. |
| `-j, --threads <N>` | 0 | Hilos a usar. 0 significa todos los núcleos. Solo .zip puede ir en paralelo. |
| `-p, --password <PASSWORD>` | — | Contraseña de un archivo cifrado (AES-256 o ZipCrypto). |

## test · t

Comprueba la integridad sin escribir en disco.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo a comprobar. |
| `-p, --password <PASSWORD>` | — | Contraseña de un archivo cifrado (AES-256 o ZipCrypto). |

## password

Reescribe un `.zip` con otra contraseña, o sin ninguna. Las entradas no se vuelven a comprimir, porque el cifrado WinZip AES cifra los bytes ya comprimidos.

| Argumento / opción | Por defecto | Descripción |
| --- | --- | --- |
| `<ARCHIVE>` | obligatorio | Archivo a reescribir (.zip). |
| `-p, --password <PASSWORD>` | — | Contraseña actual, si el archivo tiene una. |
| `--new <NEW>` | — | Contraseña nueva. Omítela para quitar el cifrado. |
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
| `1` | Cualquier error. El mensaje se imprime en la salida de error estándar como `arca: <message>`. |
</content>
