---
description: Niveles, códecs, hilos y formatos para arca create.
group: Uso de Arca
order: 4
keywords: crear comprimir nivel códec zstd deflate store hilos tar gzip xz txz lzma2 -l -c -j create compress level codec zstd deflate store threads tar gzip -l -c -j
---

# Crear archivos

```text
arca create <OUT> <INPUTS>... [-l LEVEL] [-c CODEC] [-j THREADS] [-p PASSWORD] [--hide-names]
```

| Opción | Valores | Por defecto | Descripción |
| --- | --- | --- | --- |
| `-l, --level` | store · fast · normal · best | normal | Nivel de compresión. |
| `-c, --codec` | auto · store · deflate · zstd · lzma2 | auto | Deflate en .zip, LZMA2 en .7z, .xz y .tar.xz. |
| `-j, --threads` | un número | 0 | Límite de hilos ZIP y XZ. 0 usa todos los núcleos; 7z es secuencial. |
| `-p, --password` | texto | ninguna | Cifra ZIP o 7z con AES-256. |
| `--hide-names` | indicador | desactivado | Cifra también las cabeceras 7z; exige una contraseña no vacía. |

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

## 7z

```sh
arca create copy.7z my-files/ -l normal
arca create stored.7z my-files/ -c store
arca create secret.7z my-files/ -c lzma2 -p "a password" --hide-names
```

Los niveles `store`, `fast`, `normal` y `best` seleccionan Copy o LZMA2 con nivel
1, 6 y 9. `-c store` prevalece sobre el nivel. 7z solo admite `auto`, `store` y
`lzma2`; la selección explícita Deflate/Zstandard se aplica a ZIP, no a 7z.
TAR ignora esas opciones y `.tar.gz` siempre usa gzip; `lzma2` se rechaza fuera de 7z.
La creación es secuencial independientemente de `-j`, con un bloque independiente
por fichero, sin compresión sólida. Sí se pueden leer archivos sólidos existentes.

7z incluye ficheros y directorios vacíos y rechaza enlaces simbólicos, puntos de
reanálisis y ficheros especiales. La creación usa un temporal junto al destino y
solo lo sustituye al terminar correctamente; un fallo conserva el destino anterior.
La carpeta padre del destino debe existir. Si todas las entradas están vacías, el
cifrado 7z exige `--hide-names` para que se pueda comprobar la contraseña.

## TAR y gzip

```sh
arca create copy.tar my-files/
arca create copy.tar.gz my-files/ -l best
```

La capa gzip respeta el nivel elegido. TAR no tiene dónde poner el cifrado, así que `-p` se rechaza para `.tar` y `.tar.gz`.

## XZ

```sh
arca create copy.tar.xz my-files/ -l best   # también vale .txz
arca create notes.txt.xz notes.txt          # un fichero, sin contenedor
```

Un `.xz` es un flujo comprimido, no un archivo: guarda los bytes de un único
fichero, sin nombre, fecha ni carpeta. Arca rechaza una carpeta o más de una
entrada y sugiere `.tar.xz`, que mete un TAR dentro del flujo XZ. Al extraer
`notes.txt.xz` se escribe `notes.txt`; si el fichero no lleva el sufijo, se le
añade `.out`.

Los niveles corresponden a presets de xz: `store` es el preset 0, `fast` el 1,
`normal` el 6 y `best` el 9. Incluso el preset 0 comprime con LZMA2, porque XZ
no tiene modo sin compresión. Como códec solo se admiten `auto` y `lzma2`, y
`-p` y `--hide-names` se rechazan. La salida lleva una comprobación CRC64 que
`xz` y `tar -J` verifican.

`-j` fija cuántos bloques se comprimen a la vez. Cada hilo necesita la memoria
de su preset, así que Arca usa tantos como quepan en 2 GiB (uno con `best`,
hasta 14 con `normal`) e indica cuántos ha usado. Con más de uno, el flujo se divide en bloques
independientes, lo que cuesta un poco de ratio. La lectura es un único flujo
secuencial. La creación escribe un temporal junto al destino y solo lo renombra
si termina bien.

## Qué se guarda

- Los directorios se recorren de forma recursiva en orden alfabético, así que la misma entrada siempre produce el mismo orden de entradas.
- Las rutas se guardan relativas al padre de cada entrada: `arca create a.zip ~/work/site` guarda las entradas como `site/…`.
- Se preservan las fechas de modificación.
- ZIP/TAR añaden ficheros normales y omiten enlaces simbólicos. 7z también conserva directorios vacíos y rechaza enlaces simbólicos (ver arriba).

## Cifrar al crear

```sh
arca create secret.zip folder/ -p "a password"
```

Consulta [Cifrado](encryption.md) para conocer el esquema, sus garantías y sus límites.

## Salida

```text
silesia.zip: 12 files, 202.1 MB -> 63.1 MB (68.8% smaller) in 0.282 s · 717 MB/s · 24 threads
```
