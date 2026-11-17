---
description: Niveles, códecs, hilos y formatos para arca create, incluidos los nuevos archivos RAR5.
group: Uso de Arca
order: 4
keywords: crear comprimir nivel códec zstd deflate store hilos tar gzip xz txz lzma2 bzip2 bz2 tbz2 zst tzst rar rar5 dividir partes volumen 001 sfx autoextraíble exe -l -c -j -v create compress level codec zstd deflate store threads tar gzip xz txz lzma2 rar rar5 -l -c -j
---

# Crear archivos

```text
arca create <OUT> <INPUTS>... [-l LEVEL] [-c CODEC] [-j THREADS] [-p PASSWORD] [--hide-names] [-v SIZE] [--sfx [--console]]
```

| Opción | Valores | Por defecto | Descripción |
| --- | --- | --- | --- |
| `-l, --level` | store · fast · normal · best | normal | Nivel de compresión. |
| `-c, --codec` | auto · store · deflate · zstd · lzma2 | auto | Deflate en .zip, LZMA2 en .7z, .xz y .tar.xz, bzip2 en .bz2 y .tar.bz2, Zstandard en .zst y .tar.zst, el compresor RAR en .rar (solo auto o store). |
| `-j, --threads` | un número | 0 | Límite de hilos ZIP, XZ y Zstandard. 0 usa todos los núcleos; bzip2 usa siempre uno; 7z y RAR usan todos los núcleos que permite su presupuesto de memoria. |
| `-p, --password` | texto | ninguna | Cifra ZIP o 7z con AES-256. Se rechaza en .rar. |
| `--hide-names` | indicador | desactivado | Cifra también las cabeceras 7z; exige una contraseña no vacía. |
| `-v, --volume` | un tamaño: bytes, o k, m, g | ninguno | Divide el archivo en partes de ese tamaño, `salida.zip.001`, `salida.zip.002`… No en .rar. |
| `--sfx` | indicador | desactivado | Escribe un programa de Windows que se extrae solo en lugar de un archivo. |
| `--console` | indicador | desactivado | Con `--sfx`: el programa pregunta e informa en la consola. |

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
Los ficheros se comprimen juntos en bloques sólidos de hasta 256 MiB, agrupados
por extensión como hace 7-Zip; los ejecutables x86 (`.exe`, `.dll` y similares)
van en bloques propios con el filtro BCJ, que reduce el código máquina. LZMA2 usa
tantos núcleos como quepan en 2 GiB (todos en `fast` y `normal`, uno en `best`), y
las entradas se listan en ese orden agrupado.

7z incluye ficheros y directorios vacíos y rechaza enlaces simbólicos, puntos de
reanálisis y ficheros especiales. La creación usa un temporal junto al destino y
solo lo sustituye al terminar correctamente; un fallo conserva el destino anterior.
La carpeta padre del destino debe existir. Si todas las entradas están vacías, el
cifrado 7z exige `--hide-names` para que se pueda comprobar la contraseña.

## RAR

```sh
arca create backup.rar documents/ notes.txt -l best
arca create stored.rar big.iso -c store
```

Crea un archivo RAR5 nuevo, de un solo volumen, no sólido y sin cifrar, que
UnRAR y 7-Zip oficiales abren. Los cuatro niveles eligen la fuerza del compresor
RAR; `-c store` equivale a `-l store` y cualquier otro códec explícito se
rechaza. La creación usa todos los núcleos que permite su presupuesto de memoria, diga lo que diga `-j`, guarda
ficheros y directorios vacíos y rechaza enlaces simbólicos y ficheros especiales
en lugar de seguirlos.

La salida RAR nunca se cifra (`-p` y `--hide-names` se rechazan), nunca es un
`.cbr`, nunca es sólida ni multivolumen y nunca reemplaza un fichero existente:
el archivo se prepara junto al destino, se verifica con el lector de Arca y solo
se publica si el nombre sigue libre. Los RAR/CBR existentes siguen siendo de
solo lectura. Los límites y la lista completa de rechazos están en la
[guía de RAR](rar.md#crear-archivos-rar5).

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

## bzip2 y Zstandard

```sh
arca create copy.tar.bz2 my-files/          # también valen .tbz2 y .tbz
arca create copy.tar.zst my-files/ -j 8     # también vale .tzst
arca create notes.txt.bz2 notes.txt         # un fichero, sin contenedor
arca create notes.txt.zst notes.txt
```

Funcionan como XZ: un `.bz2` o un `.zst` guarda un único fichero, y una carpeta
o varias entradas necesitan la variante TAR. Al leer, la firma decide el códec
y el contenido decide si lleva un TAR dentro, diga lo que diga el nombre.

bzip2 comprime con un solo hilo. `store` y `fast` usan sus bloques de 100 kB, y
`best` los de 900 kB. Como códec solo admite `auto`. Cada bloque y el flujo
entero llevan un CRC que `bzip2 -t` y `tar -j` verifican, y los flujos
concatenados, como los que escribe `pbzip2`, se leen como uno solo.

Zstandard usa los mismos niveles que Zstandard dentro de un ZIP y tantos hilos
como pida `-j`; admite `auto` y `zstd`. Arca escribe una suma de comprobación
del contenido y, para un único fichero, su tamaño, así que `list` lo muestra
sin descomprimir. Al leer acepta ventanas de hasta 1 GiB, el mismo techo que el
diccionario de XZ, así que las tramas hechas con `zstd --long=31` se rechazan.
Zstandard necesita la característica `codecs-native`, activa por defecto.

bzip2 nunca guarda el tamaño original, y un flujo Zstandard escrito a una
tubería puede no guardarlo tampoco, así que `list` muestra 0 para ellos. Los
datos sobrantes al final que no son otro flujo son un error.

## Dividir en partes

```sh
arca create copia.zip fotos/ -v 700m       # copia.zip.001, copia.zip.002, …
arca create copia.7z fotos/ -v 4092m       # partes que caben en FAT32
arca create registros.tar.xz registros/ -v 100m
```

`-v` corta los bytes del archivo terminado en partes del tamaño indicado, igual
que el `-v` de 7-Zip: todas las partes menos la última tienen exactamente ese
tamaño y no se les añade nada, así que 7-Zip y NanaZip también abren el
conjunto. El tamaño es un número de bytes, o uno terminado en `k`, `m` o `g`
para KiB, MiB y GiB. Funciona con ZIP, 7z, todas las variantes de TAR y los
flujos de un solo fichero; `.rar` se rechaza, porque RAR tiene volúmenes
propios que Arca no escribe. Un conjunto tiene como mucho 9999 partes: un
tamaño demasiado pequeño para eso es un error y no deja nada detrás.

Las partes se escriben directamente en su carpeta final, una tras otra, así que
el archivo nunca existe allí como un único fichero grande; por eso un conjunto
de 4 GB cabe en una memoria FAT32. Se escriben con nombres temporales y solo
toman su nombre real cuando el conjunto está completo, de modo que una parada o
un error no dejan un conjunto a medias. `create` se niega a sustituir un `.001`
existente: borra antes las partes viejas o elige otro nombre. Dar
`copia.zip.001` como nombre de salida equivale a dar `copia.zip`.

Para leer un conjunto basta con abrir cualquiera de sus partes:
`arca list copia.zip.002`, `arca test` y `arca extract` encuentran las demás en
la misma carpeta y las leen como un solo archivo. Tienen que estar todas,
numeradas sin huecos, y todas menos la última deben medir lo mismo; el error
nombra la parte que falta o que es demasiado corta. Un archivo dividido es de
solo lectura: `arca password` y las órdenes de edición de la ventana lo
rechazan. Los conjuntos multivolumen de RAR (`.part1.rar`) y las imágenes ISO
divididas no se unen de esta forma.

## Programas que se extraen solos

```sh
arca create instalar.exe mis-ficheros/ --sfx
arca create instalar.exe mis-ficheros/ --sfx --console
```

`--sfx` escribe un programa de Windows que lleva dentro un ZIP con las
entradas. Al ejecutarlo muestra un selector de carpeta y extrae en la carpeta
elegida, con las mismas comprobaciones de rutas que `arca extract`: ninguna
entrada puede acabar fuera de esa carpeta y un fichero cuya suma de
comprobación no coincide no se escribe. Los ficheros que ya estén allí se
sustituyen. Con `--console` el programa pide la carpeta en la consola desde la
que se lanzó y propone por defecto una carpeta junto a él, con su nombre. Los
dos tipos aceptan también `-o CARPETA` para extraer ahí sin preguntar, pensado
para scripts.

El programa es el `arca-sfx.exe` que viene con Arca, instalado junto a
`arca.exe`; `arca create` lo copia y le añade el ZIP y una cola corta que dice
dónde empieza el ZIP. Si no está, `create` dice dónde lo buscó
(`cargo build -p arca-sfx` lo compila desde el código fuente, y `ARCA_SFX_STUB`
puede señalar otra copia). El ZIP de dentro usa Deflate o Store, así que
`-c zstd` y `-c lzma2` se rechazan, y `-p`, `--hide-names` y `-v` no se pueden
combinar con `--sfx`. Los autoextraíbles se hacen para Windows; los paquetes de
Linux y macOS no incluyen el programa.

## Qué se guarda

- Los directorios se recorren de forma recursiva en orden alfabético, así que la misma entrada siempre produce el mismo orden de entradas.
- Las rutas se guardan relativas al padre de cada entrada: `arca create a.zip ~/work/site` guarda las entradas como `site/…`.
- Se preservan las fechas de modificación.
- ZIP/TAR añaden ficheros normales y omiten enlaces simbólicos. 7z y RAR también conservan directorios vacíos y rechazan enlaces simbólicos (ver arriba).

## Cifrar al crear

```sh
arca create secret.zip folder/ -p "a password"
```

Consulta [Cifrado](encryption.md) para conocer el esquema, sus garantías y sus límites.

## Salida

```text
silesia.zip: 12 files, 202.1 MB -> 63.1 MB (68.8% smaller) in 0.282 s · 717 MB/s · 24 threads
```
