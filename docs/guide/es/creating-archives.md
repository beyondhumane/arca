---
description: Niveles, códecs, hilos y formatos para arca create.
group: Uso de Arca
order: 4
keywords: crear comprimir nivel códec zstd deflate store hilos tar gzip -l -c -j create compress level codec zstd deflate store threads tar gzip -l -c -j
---

# Crear archivos

```text
arca create <OUT> <INPUTS>... [-l LEVEL] [-c CODEC] [-j THREADS] [-p PASSWORD]
```

| Opción | Valores | Por defecto | Descripción |
| --- | --- | --- | --- |
| `-l, --level` | store · fast · normal · best | normal | Nivel de compresión. |
| `-c, --codec` | auto · store · deflate · zstd | auto | Compresor. auto usa Deflate en .zip por compatibilidad. |
| `-j, --threads` | un número | 0 | Hilos a usar. 0 significa todos los núcleos. |
| `-p, --password` | texto | — | Cifra con AES-256. Solo .zip. |

## Niveles

```sh
arca create copy.zip my-files/ -l store    # sin compresión, lo más rápido
arca create copy.zip my-files/ -l fast
arca create copy.zip my-files/ -l normal   # el valor por defecto
arca create copy.zip my-files/ -l best     # el resultado más pequeño
```

## Códecs

`auto` elige Deflate para `.zip`, porque un zip existe para que cualquiera pueda abrirlo. Pide Zstandard explícitamente cuando controlas el lado receptor:

```sh
arca create copy.zip my-files/ -c zstd
```

> [!WARNING]
> **Zstandard dentro de ZIP**
>
> Zstandard es el método ZIP 93. Está registrado en la especificación, pero el `unzip` clásico aún no puede leerlo, así que úsalo cuando sepas quién abrirá el archivo. Será el valor por defecto en cuanto Arca tenga un formato nativo.

## Hilos

Arca usa todos los núcleos por defecto. Fija el número con `-j`, por ejemplo en ejecutores de CI compartidos:

```sh
arca create copy.zip my-files/ -j 8
arca create copy.zip my-files/ -j 0    # todos los núcleos (el valor por defecto)
```

El escalado midió 1.79× con dos hilos, 90% de eficiencia, frente a un requisito de diseño (R3) de al menos 1.6×. Consulta [benchmarks](benchmarks.md) para ver el comando.

## TAR y gzip

```sh
arca create copy.tar my-files/
arca create copy.tar.gz my-files/ -l best
```

La capa gzip respeta el nivel elegido. TAR no tiene dónde poner el cifrado, así que `-p` se rechaza para `.tar` y `.tar.gz`.

## Qué se guarda

- Los directorios se recorren de forma recursiva en orden alfabético, así que la misma entrada siempre produce el mismo orden de entradas.
- Las rutas se guardan relativas al padre de cada entrada: `arca create a.zip ~/work/site` guarda las entradas como `site/…`.
- Se preservan las fechas de modificación.
- Solo se añaden archivos normales. Los enlaces simbólicos se omiten por ahora (ver la [hoja de ruta](roadmap.md)).

## Cifrar al crear

```sh
arca create secret.zip folder/ -p "a password"
```

Consulta [Cifrado](encryption.md) para conocer el esquema, sus garantías y sus límites.

## Salida

```text
silesia.zip: 12 files, 202.1 MB -> 63.1 MB (68.8% smaller) in 0.282 s · 717 MB/s · 24 threads
```
