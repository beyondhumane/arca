---
description: Niveles, códecs, hilos y formatos para arca create, incluidos los nuevos archivos RAR5.
group: Uso de Arca
order: 4
keywords: crear comprimir nivel códec zstd deflate store hilos tar gzip xz txz lzma2 lzma ppmd bcj arm64 delta filtro bzip2 bz2 tbz2 zst tzst rar rar5 sólido recuperación part1 dividir partes volumen 001 sfx autoextraíble exe -l -c -j -v create compress level codec zstd deflate store threads tar gzip xz txz lzma2 rar rar5 -l -c -j
---

# Crear archivos

[English](../creating-archives.md) | **Español**

```text
arca create <OUT> <INPUTS>... [-l LEVEL] [-c CODEC] [-j THREADS] [-p PASSWORD] [--hide-names] [-v SIZE] [--solid] [--recovery PERCENT] [--sfx [--console]]
```

| Opción | Valores | Por defecto | Descripción |
| --- | --- | --- | --- |
| `-l, --level` | store · fast · normal · best | normal | Nivel de compresión. |
| `-c, --codec` | auto · store · deflate · zstd · lzma2 · lzma · ppmd · bzip2 | auto | Deflate en .zip, LZMA2 en .7z, .xz y .tar.xz, bzip2 en .bz2 y .tar.bz2, Zstandard en .zst y .tar.zst, el compresor RAR en .rar (solo auto o store). .7z admite además lzma, ppmd, bzip2, deflate y store, véase [7z](#7z). |
| `-j, --threads` | un número | 0 | Límite de hilos ZIP, XZ y Zstandard. 0 usa todos los núcleos; bzip2 usa siempre uno; 7z con LZMA2 y RAR usan todos los núcleos que permite su presupuesto de memoria, y 7z con otro método, uno. |
| `-p, --password` | texto | ninguna | Cifra ZIP, 7z o RAR con AES-256. |
| `--hide-names` | indicador | desactivado | Cifra también las cabeceras 7z o RAR, para ocultar los nombres; exige una contraseña no vacía. |
| `-v, --volume` | un tamaño: bytes, o k, m, g | ninguno | Divide el archivo en partes de ese tamaño, `salida.zip.001`, `salida.zip.002`…; un .rar se escribe en cambio en volúmenes RAR `salida.part1.rar`, `salida.part2.rar`… |
| `--solid` | indicador | desactivado | Solo RAR: comprime los ficheros como un único flujo. |
| `--recovery` | de 1 a 100, con o sin `%` | ninguno | Solo RAR: añade un registro de recuperación de ese porcentaje. |
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
arca create notes.7z logs/ -c ppmd -l best
arca create secret.7z my-files/ -c lzma2 -p "a password" --hide-names
```

`-c` elige el método; el nivel fija sus parámetros como hace 7-Zip, y
`-l store` o `-c store` guardan sin comprimir aunque se pida otra cosa:

| `-c` | Método | Para qué | Parámetros por nivel 1 · 5 · 6 · 9 |
| --- | --- | --- | --- |
| `auto`, `lzma2` | LZMA2 (por defecto) | cualquier cosa; el único que usa varios núcleos | diccionario 1 · 8 · 8 · 64 MiB |
| `lzma` | LZMA | lectores anteriores a LZMA2 | diccionario 1 · 8 · 8 · 64 MiB |
| `ppmd` | PPMd | texto: registros, código fuente, documentos | orden 4 · 6 · 8 · 32, modelo 1 · 16 · 32 · 192 MiB |
| `bzip2` | BZip2 | rapidez con una reducción razonable | bloques de 100 · 500 · 600 · 900 kB |
| `deflate` | Deflate | el más rápido al descomprimir | nivel zlib 1 · 5 · 6 · 9 |
| `store` | Copy | datos que ya vienen comprimidos | ninguno |

BZip2 necesita una compilación con la característica por defecto
`codecs-native`. Zstandard no se ofrece para 7z porque el propio 7-Zip no lo lee
ahí; `-c zstd` solo vale para ZIP, TAR ignora la elección de códec y `.tar.gz`
siempre usa gzip.

La memoria se acota igual que los hilos de LZMA2. LZMA2 usa tantos núcleos como
quepan en 2 GiB (todos en `fast` y `normal`, uno en `best`); los demás métodos
usan uno. Los diccionarios LZMA y los modelos PPMd se reducen al tamaño del
bloque que comprimen, de modo que un archivo pequeño nunca pide 64 MiB a quien lo
descomprime. El modelo PPMd no pasa de 192 MiB, el máximo nivel del propio
7-Zip, por debajo de los 256 MiB por codificador que acepta el lector de Arca:
todo lo que Arca escribe lo puede leer.

Los ficheros se comprimen juntos en bloques sólidos de hasta 256 MiB, un grupo
por filtro, ordenados por extensión como hace 7-Zip. Los primeros bytes de cada
fichero eligen su filtro, como en 7-Zip: los programas de Windows (PE), Linux
(ELF) y macOS (Mach-O) llevan el filtro de saltos de su máquina, x86/x64 (BCJ),
ARM, ARM-Thumb, ARM64, PowerPC, SPARC o IA-64, y el audio WAV PCM lleva Delta
con el tamaño de su trama de muestras. Un programa cuya cabecera no dice nada se
juzga por su extensión (`.exe`, `.dll` y similares llevan BCJ). El código RISC-V
no lleva filtro. El filtro ARM64 exige 7-Zip 23.01 o posterior para descomprimir;
los 7-Zip anteriores y p7zip 16.02 no pueden. Las entradas se listan en ese orden
agrupado.

Al editar un 7z en la ventana (borrar, renombrar o añadir entradas), cada bloque
que conserva algo se vuelve a escribir con su propio método y sus filtros, al
nivel elegido; los bloques sin comprimir siguen sin comprimir. Los ficheros que
se añaden toman el método del primer bloque del archivo. Un codificador que Arca
no sabe volver a escribir se omite, y LZMA2 sustituye al método que se pierda.

7z incluye ficheros y directorios vacíos y rechaza enlaces simbólicos, puntos de
reanálisis y ficheros especiales. La creación usa un temporal junto al destino y
solo lo sustituye al terminar correctamente; un fallo conserva el destino anterior.
La carpeta padre del destino debe existir. Si todas las entradas están vacías, el
cifrado 7z exige `--hide-names` para que se pueda comprobar la contraseña.

## RAR

```sh
arca create backup.rar documents/ notes.txt -l best
arca create stored.rar big.iso -c store
arca create privado.rar documents/ -p secreto --hide-names
arca create fotos.rar fotos/ --solid -v 700m --recovery 3
```

Crea un archivo RAR5 nuevo que WinRAR, UnRAR y 7-Zip oficiales abren. Los
cuatro niveles eligen la fuerza del compresor RAR; `-c store` equivale a
`-l store` y cualquier otro códec explícito se rechaza. La creación usa todos
los núcleos que permite su presupuesto de memoria, diga lo que diga `-j`, guarda
ficheros y directorios vacíos y rechaza enlaces simbólicos y ficheros especiales
en lugar de seguirlos.

El resto es lo que ofrece WinRAR al crear un archivo:

- `--solid` comprime los ficheros como un único flujo, como `-s` de WinRAR.
- `-p` cifra cada fichero con AES-256, como `-p` de WinRAR; `--hide-names`
  cifra además las cabeceras, como `-hp`, de modo que sin la contraseña ni
  siquiera se pueden listar los nombres.
- `-v TAMAÑO` escribe un conjunto de volúmenes RAR, como `-v`:
  `fotos.part1.rar`, `fotos.part2.rar`… (`part01` a partir de diez), cada uno
  de como mucho TAMAÑO bytes, con el registro de recuperación y las cabeceras
  incluidos, y de al menos 64 KiB. Un conjunto que cabe en un volumen conserva
  el nombre indicado. Son los volúmenes propios de RAR, no partes `.001`.
- `--recovery PORCENTAJE` añade un registro de recuperación de entre el 1 y el
  100 %, en cada volumen de un conjunto, como `-rr`.
  [`arca repair`](cli-reference.md#repair) y WinRAR reconstruyen con él los
  datos dañados. No se escriben volúmenes de recuperación (`.rev`).

Se combinan libremente. La salida RAR nunca es un `.cbr`, nunca reemplaza un
fichero existente ni se mezcla con volúmenes del mismo nombre: el archivo, o el
conjunto entero, se prepara junto al destino, se abre y se descodifica con el
lector de Arca (con la contraseña) y solo se publica si todos los nombres
siguen libres. Los RAR/CBR existentes siguen siendo de solo lectura. Los
límites y la lista completa de rechazos están en la
[guía de RAR](rar.md#crear-archivos-rar5).

## TAR y gzip

```sh
arca create copy.tar my-files/
arca create copy.tar.gz my-files/ -l best
arca create access.log.gz access.log       # un fichero, sin contenedor
```

La capa gzip respeta el nivel elegido. TAR no tiene dónde poner el cifrado, así que `-p` se rechaza para `.tar`, `.tar.gz` y `.gz`.

Un `.gz` a secas se comporta como un [`.xz`](#xz): guarda un único fichero, y
una carpeta o varias entradas necesitan `.tar.gz`. Solo se aceptan los códecs
`auto` y `deflate`. Al leer se unen todos los miembros, como los escriben
`gzip`, `pigz` y `cat a.gz b.gz`, y se comprueban el CRC-32 y la longitud de
cada uno; los datos finales que no son otro miembro son un error. `.tar.gz` y
`.tgz` se leen siempre como TAR. Un `.gz` a secas, o un fichero gzip sin
sufijo, se mira por dentro: un TAR al principio lo convierte en `.tar.gz`, y
cualquier otra cosa es un único fichero que se llama como el archivo sin
`.gz`, así que `access.log.gz` se extrae como `access.log`. El nombre que pueda
guardar la cabecera gzip no se usa. gzip solo registra el tamaño del último
miembro, módulo 4 GiB, así que `list` muestra 0 como tamaño.

En Linux y macOS, un enlace simbólico dentro de las carpetas que se archivan entra en un TAR (`.tar`, `.tar.gz`, `.tar.xz`) como enlace, después de los ficheros normales, con exactamente el destino que tenía. Nunca se sigue, así que un enlace a una carpeta de fuera no la arrastra. Los nombres y destinos que no caben en una cabecera ustar van en un registro pax `path` o `linkpath` delante de la entrada; Arca también lee los nombres largos GNU `L`/`K`, pero nunca los escribe. Si un enlace es seguro de recrear se decide al extraer (consulta [Enlaces en archivos TAR](listing-extracting.md#enlaces-en-archivos-tar)). En Windows los enlaces se dejan fuera, como antes.

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

`.bz2` y `.tar.bz2` están siempre incluidos, también con `--no-default-features`, y no necesitan `codecs-native`; el método BZip2 de [7z](#7z) es aparte. Funcionan como XZ: un `.bz2` o un `.zst` guarda un único fichero, y una carpeta
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
flujos de un solo fichero; un `.rar` no se corta así, sino que se escribe en
volúmenes RAR (véase [RAR](#rar)). Un conjunto tiene como mucho 9999 partes: un
tamaño demasiado pequeño para eso es un error y no deja nada detrás.

Las partes se escriben directamente en su carpeta final, una tras otra, así que
el archivo nunca existe allí como un único fichero grande; por eso un conjunto
de 4 GB cabe en una memoria FAT32. Se escriben con nombres temporales y solo
toman su nombre real cuando el conjunto está completo, de modo que una parada o
un error no dejan un conjunto a medias. `create` se niega a sustituir un `.001`
existente: borra antes las partes viejas o elige otro nombre. Dar
`copia.zip.001` como nombre de salida equivale a dar `copia.zip`. Cuando la
ventana sustituye un conjunto, las partes viejas se renombran primero a
`.arca-old` y solo se borran cuando todas las nuevas están en su sitio; si una
no se puede sustituir, por ejemplo porque otro programa la tiene abierta, las
partes nuevas se quitan y el conjunto viejo vuelve a quedar como estaba. Una
parte `.arca-old` que deja una ejecución cortada a medias puede ser la única
copia del conjunto viejo, así que ese conjunto no se vuelve a escribir hasta
que se aparta.

Para leer un conjunto basta con abrir cualquiera de sus partes:
`arca list copia.zip.002`, `arca test` y `arca extract` encuentran las demás en
la misma carpeta y las leen como un solo archivo. Tienen que estar todas,
numeradas sin huecos, y todas menos la última deben medir lo mismo; el error
nombra la parte que falta o que es demasiado corta. Los nombres de las partes
se comparan como los compara el sistema de ficheros, así que en Windows y macOS
`COPIA.ZIP.001` encuentra `copia.zip.002`. Un fichero numerado sin ninguna otra
parte de su conjunto al lado, como un `copia.zip.100` guardado por separado, se
abre como un fichero suelto. Un archivo dividido es de solo lectura:
`arca password` y las órdenes de edición de la ventana lo rechazan. Los
conjuntos multivolumen de RAR (`.part1.rar`) y las imágenes ISO divididas no se
unen de esta forma.

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
sustituyen. Si la extracción falla, las carpetas que creó y dejó vacías se
vuelven a quitar. Con `--console` el programa pide la carpeta en la consola
desde la que se lanzó y propone por defecto una carpeta junto a él, con su
nombre. Los dos tipos aceptan también `-o CARPETA` para extraer ahí sin
preguntar, pensado para scripts.

El programa es el `arca-sfx.exe` que viene con Arca, instalado junto a
`arca.exe`; `arca create` lo copia y le añade el ZIP y una cola corta que dice
dónde empieza el ZIP. Si no está, `create` dice dónde lo buscó
(`cargo build -p arca-sfx` lo compila desde el código fuente, y `ARCA_SFX_STUB`
puede señalar otra copia). El ZIP de dentro usa Deflate o Store, así que
`-c zstd` y `-c lzma2` se rechazan, y `-p`, `--hide-names` y `-v` no se pueden
combinar con `--sfx`. Lo que sale es un programa, así que un nombre con la
extensión de un archivo comprimido, como `.zip` o `.7z`, se rechaza; usa
`.exe`. Los autoextraíbles se hacen para Windows; los paquetes de Linux y macOS
no incluyen el programa.

El ZIP cuenta sus desplazamientos desde el principio del programa, como deja
`zip -A` un autoextraíble, así que herramientas ZIP como `unzip -t instalar.exe`
o el `ZipFile.OpenRead` de .NET leen el propio programa como un ZIP, y también
`arca list`, `arca test` y `arca extract`.

## Qué se guarda

- Los directorios se recorren de forma recursiva en orden alfabético, así que la misma entrada siempre produce el mismo orden de entradas.
- Las rutas se guardan relativas al padre de cada entrada: `arca create a.zip ~/work/site` guarda las entradas como `site/…`.
- Se preservan las fechas de modificación.
- ZIP añade ficheros normales y omite los enlaces simbólicos sin avisar. TAR añade ficheros normales y, en Linux y macOS, guarda los enlaces simbólicos como enlaces sin seguirlos (ver arriba). 7z y RAR también conservan directorios vacíos y rechazan enlaces simbólicos (ver arriba).

## Cifrar al crear

```sh
arca create secret.zip folder/ -p "a password"
```

Consulta [Cifrado](encryption.md) para conocer el esquema, sus garantías y sus límites.

## Salida

```text
silesia.zip: 12 files, 202.1 MB -> 63.1 MB (68.8% smaller) in 0.282 s · 717 MB/s · 24 threads
```
