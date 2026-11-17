---
description: Cada medición, la máquina detrás y los casos donde Arca pierde.
group: Referencia
order: 14
keywords: rendimiento velocidad benchmarks nanazip 7-zip zip números puntuación códecs hilos rar rar5 winrar performance speed benchmarks nanazip 7-zip zip numbers rating codecs threads
---

# Benchmarks

Estos números vienen de máquinas concretas y no deben leerse como una ventaja general. Cada uno trae su método, incluidos los casos donde Arca pierde.

## Mide tu propia máquina

`arca bench --codecs` mide cada códec con el que Arca escribe en la máquina donde se ejecuta, como el benchmark de 7-Zip (Tools → Benchmark). No necesita ficheros ni toca el disco: los datos se generan en memoria. La aplicación de escritorio ejecuta la misma prueba desde **Prueba de rendimiento…** en el menú **Más**; consulta [la aplicación de escritorio](desktop-app.md#prueba-de-rendimiento).

```sh
arca bench --codecs
arca bench --codecs --threads 8 --seconds 3 --size 32
arca bench --codecs --json
```

Esta página no publica cifras suyas. Su salida termina siempre con la orden que la reproduce; conserva esa línea junto a cualquier número que compartas.

Cómo mide:

- **Los datos.** `--size` MiB (16 por defecto, y al menos 1 MiB por hilo) generados a partir de una semilla fija, así que todas las máquinas y todas las ejecuciones reciben los mismos bytes. De cada cuatro tramos de 64 KiB, tres son texto parecido a prosa, hecho con un vocabulario fijo en el que las palabras comunes aparecen mucho más que las raras, y el cuarto son bytes aleatorios.
- **Los códecs.** Todos trabajan al nivel 6 (normal), el predeterminado de `arca create`, con los mismos escritores y lectores que usa Arca: Store, Deflate y Zstandard como entradas de un ZIP, LZMA2 como un flujo `.xz` por trozo, bzip2 como un flujo `.bz2` por trozo y RAR5 como un archivo RAR guardado en memoria. Un códec que la compilación no tiene aparece como `not in this build`.
- **Los hilos.** Los datos se cortan en trozos de 1 MiB y cada uno se comprime por su cuenta, como lo haría una entrada de un ZIP. Con un hilo se trabaja un trozo cada vez; con `--threads N` (todos los núcleos por defecto), N a la vez. Las dos columnas hacen exactamente el mismo trabajo, así que solo cambia el paralelismo. LZMA2 no usa más hilos de los que caben en los 2 GiB que `arca create` da a un `.tar.xz`, y la tabla lo indica cuando son menos. `arca create` escribe un `.bz2` en un solo hilo, así que la columna de bzip2 con N hilos muestra lo que hace el procesador con trozos independientes, no lo rápido que va `arca create x.bz2`.
- **El tiempo.** Cada medición repite sus pasadas hasta que pasan `--seconds` segundos (1 por defecto; siempre se hace al menos una) y da la mediana. Las velocidades son MB/s de datos sin comprimir en los dos sentidos, con 1 MB = 10^6 bytes. La relación es el tamaño comprimido entre el original, cabeceras del archivo incluidas.
- **La comprobación.** Cada trozo descomprimido se compara con el original, fuera de la parte cronometrada. Una diferencia detiene la prueba con un error.

### La puntuación

La puntuación es la media geométrica de seis velocidades: compresión y descompresión de Deflate, LZMA2 y bzip2. Es decir, con `c` para la compresión y `d` para la descompresión, en MB/s:

```text
puntuación = (c_Deflate × d_Deflate × c_LZMA2 × d_LZMA2 × c_bzip2 × d_bzip2) ^ (1/6)
```

Se calcula una vez para un hilo y otra para `--threads`. Se usan esos tres códecs porque todas las compilaciones los tienen: a las que no llevan los códecs nativos les falta Zstandard, a las que no llevan RAR les falta RAR5, y Store solo mide el ancho de banda de la memoria. Una media geométrica evita que un códec rápido tape a uno lento: duplicar cualquiera de las seis velocidades sube la puntuación el mismo 12 %, sea cual sea.

Dos puntuaciones solo se comparan si vienen de la misma versión de Arca, el mismo tipo de compilación y las mismas opciones. La primera línea de la salida dice qué compilación es: `native codecs` usa libdeflate para Deflate, `pure Rust codecs` usa zlib-rs, y las dos no van a la misma velocidad. La puntuación con varios hilos depende además de `--threads`.

### Memoria y parada

La prueba guarda los datos, una copia comprimida de ellos y dos trozos por hilo: unas dos veces `--size` más 2 MiB por hilo. La salida imprime esa cifra. Encima va el estado de los códecs, y el mayor es el de los codificadores LZMA2: los 94 MiB que xz(1) documenta para el nivel 6, en como mucho tantos hilos como caben en 2 GiB, que la salida también imprime como cota superior.

Ctrl+C detiene la prueba entre pasadas, así que la espera es como mucho una pasada, y la más larga es un trozo de LZMA2 en un hilo. Arca imprime entonces la tabla con lo que había medido, dice que se detuvo antes del final y sale con el estado 130. En una compilación sin RAR el manejador no se instala y Ctrl+C termina el proceso en el acto, sin perder nada porque la prueba no escribe nada.

## Requisitos

Ryzen 9 5900X, Linux. Todo lo de esta sección y la siguiente se reproduce con un script, que descarga el [corpus Silesia](https://sun.aei.polsl.pl/~sdeor/index.php?page=silesia), lo fija por hash e imprime la máquina y las versiones de las herramientas con las que se ejecutó:

```sh
cargo build --release
nix-shell -p hyperfine zip unzip _7zz --run 'bash bench.sh'
```

| Requisito | Objetivo | Medido | Otros |
| --- | --- | --- | --- |
| R1 · arranque en frío, listar un zip de una entrada | < 15 ms | **0.8 ms** | unzip 2.0 · 7z 2.5 |
| R2 · listar 6000 entradas | < 200 ms | **3.1 ms** | unzip 18.6 · 7z 71.8 |
| R3 · escalado a 2 hilos | ≥ 1.6× | **1.79×** | 90% de eficiencia |

```sh
hyperfine -N --warmup 20 'arca list tiny.zip' 'unzip -l tiny.zip' '7zz l tiny.zip'
hyperfine -N --warmup 5 'arca list list-6000.zip' 'unzip -l list-6000.zip' '7zz l list-6000.zip'
hyperfine -N --prepare 'rm -f out.zip' \
  'taskset -c 0,1 arca create out.zip corpus -c deflate -j 1' \
  'taskset -c 0,1 arca create out.zip corpus -c deflate -j 2'
```

## Compresión

El corpus Silesia, 211.9 MB divididos en 120 archivos iguales, fijado a dos núcleos físicos.

| Herramienta | Tiempo | Tamaño |
| --- | --- | --- |
| **Arca, zstd** | **436 ms** | 66.87 MB |
| Arca, deflate | 1012 ms | 67.62 MB |
| zip -6 | 5746 ms | 68.34 MB |
| 7-Zip zip, -mx5 | 7877 ms | **65.81 MB** |

```sh
hyperfine -N --prepare 'rm -f out.zip' \
  'taskset -c 0,1 arca create out.zip corpus -c zstd' \
  'taskset -c 0,1 arca create out.zip corpus -c deflate' \
  'taskset -c 0,1 zip -qr -6 out.zip corpus' \
  'taskset -c 0,1 7zz a -tzip -mx5 out.zip corpus'
```

Con Zstandard, Arca fue 13.2× más rápido que `zip` y 18.1× más rápido que 7-Zip; 7-Zip siguió escribiendo el archivo más pequeño, un 1.6% por debajo de Arca. Con Deflate, Arca fue 5.7× más rápido que `zip` y su archivo quedó un 1.1% más pequeño.

> [!NOTE]
> **Por qué 120 archivos**
>
> Cada entrada se comprime en un hilo. Los doce archivos propios de Silesia están dominados por uno de 49 MB, así que con ellos dos hilos solo llegan a 1.56×.

### Deflate contra deflate en Windows 11

16 hilos, mejor de 3.

| Corpus | Arca | 7-Zip -mx5 | Tamaño Arca | Tamaño 7-Zip |
| --- | --- | --- | --- | --- |
| 5358 archivos fuente, 54.8 MB | 0.710 s | **0.627 s** | 13,729,308 | 13,747,705 |
| 16 archivos, 287 MB | **0.402 s** | 2.285 s | 56,757,284 | 54,753,868 |

```sh
arca create c1.zip src
7z a -tzip -mx5 c2.zip src
```

Con archivos grandes, Arca comprimió 5.7× más rápido a costa de un 3.7% más de tamaño; con muchos archivos pequeños, 7-Zip fue algo mejor con el mismo tamaño. El ganador depende del corpus. Esas cifras son anteriores al trabajo con archivos pequeños de la sección siguiente.

### Muchos archivos pequeños en Windows 11

Ryzen 7 3700X (8 núcleos, 16 hilos), Windows 11, con la protección en tiempo real de Microsoft Defender activa, un SSD SATA sin DRAM (Kingston A400) y caché caliente. En la máquina se compilaban otras cosas a la vez, así que los tiempos de reloj se mueven: cada fila es la mediana de las pasadas en orden A B B A, con el rango entre corchetes, y también se da el tiempo de CPU del proceso (usuario más núcleo), que se mueve menos. «Antes» es Arca 0.10.0 en el commit `4b1d67b`.

El corpus son 50,000 archivos de 0 a 16,384 bytes en 100 carpetas, 410,069,593 bytes en total, tres partes de texto parecido a prosa por una de bytes aleatorios, generados a partir de una semilla fija:

```sh
cargo run --release -p arca-bench --example small_files -- C:/b/small
```

| Crear, 16 hilos, 16 pasadas | Reloj | CPU | Memoria máxima | Archivo |
| --- | --- | --- | --- | --- |
| Antes | 5.003 s [4.420–7.743] | 19.75 s | 463 MiB | 252,139,697 |
| **Después** | **1.246 s** [1.067–2.490] | **12.33 s** | **59 MiB** | **251,139,697** |

| Crear, `-j 1`, 6 pasadas | Reloj | CPU | Memoria máxima |
| --- | --- | --- | --- |
| Antes | 14.284 s [12.426–24.841] | 14.02 s | 64 MiB |
| **Después** | **11.504 s** [10.039–12.117] | **11.75 s** | **34 MiB** |

```powershell
powershell -File benchmarks\time.ps1 -Exes arca-0.10.0.exe,arca.exe `
  -ArgLine "create C:\b\out.zip C:\b\small" -Runs 16 -Clean C:\b\out.zip
powershell -File benchmarks\time.ps1 -Exes arca-0.10.0.exe,arca.exe `
  -ArgLine "create C:\b\out.zip C:\b\small -j 1" -Runs 6 -Clean C:\b\out.zip
```

Dónde se iba el tiempo y qué cambió:

- **El recorrido.** A cada archivo se le pedían los metadatos dos veces antes de leerlo, y en Windows cada petición abre y cierra el archivo, todo en un solo hilo. El tamaño y la fecha salen ahora del listado de la carpeta, que ya los tiene.
- **El escritor.** La cabecera local de cada entrada se corregía después de sus datos, con dos saltos atrás y uno adelante, y cada salto vaciaba el búfer de salida. Una entrada comprimida en memoria recibe ahora su cabecera completa, sin el campo Zip64 de 20 bytes que nunca necesita; ese campo es lo que hace el archivo 1,000,000 de bytes más pequeño.
- **El compresor.** Un compresor de libdeflate o de Zstandard nuevo para cada archivo costaba más que comprimir un archivo pequeño. Ahora cada hilo conserva el suyo.
- **El orden.** Se comprimían todos los archivos antes de escribir el primero. Ahora cada archivo se escribe en cuanto le toca, mientras los siguientes aún se comprimen. Como antes, hay como mucho 32 MiB de datos de archivos por hilo en vuelo, y con archivos pequeños el escritor no se queda atrás, así que se retiene mucho menos.

| Extraer, 16 hilos, 6 pasadas | Reloj | CPU |
| --- | --- | --- |
| Antes | 16.948 s [11.255–24.115] | 34.50 s (núcleo 31.89 s) |
| Después | 12.310 s [10.614–19.535] | 33.09 s (núcleo 30.38 s) |

```powershell
arca create C:\b\small.zip C:\b\small
powershell -File benchmarks\time.ps1 -Exes arca-0.10.0.exe,arca.exe `
  -ArgLine "extract C:\b\small.zip -o C:\b\out" -Runs 6 -Clean C:\b\out
```

Extraer es tiempo del sistema de archivos, no de Arca: de 33 a 35 segundos de CPU, unos 30 se van en el núcleo creando 50,000 archivos, y los dos rangos se solapan. La línea de órdenes ya leía con un lector con búfer por hilo; ahora crea cada archivo nuevo en una sola llamada en vez de comprobar antes si existe, y una escritura que falla ya no deja un archivo a medias. Aquí no se afirma ninguna mejora de velocidad.

La aplicación de escritorio volvía a abrir el archivo para cada entrada que extraía, detrás de un búfer nuevo de 256 KiB que el salto a la entrada desechaba. Contado en un hilo con el ejemplo que reproduce ese patrón, son 50,001 aperturas y 26,218,015,557 bytes leídos para este archivo de 251 MB, frente a 2 aperturas y 251,341,637 bytes ahora. La ventana en sí no se cronometró.

```sh
cargo run --release -p arca-zip --example read_amplification -- C:/b/small.zip baseline
cargo run --release -p arca-zip --example read_amplification -- C:/b/small.zip both
```

### Tamaño en Deflate frente al ZIP de WinRAR

Los tamaños no dependen de la máquina. 7-Zip no estaba instalado, así que WinRAR 7.23, que también escribe ZIP con Deflate, ocupa su lugar. Bytes del archivo, todo incluido:

| Corpus | Arca -l 6 | Arca -l 8 | Arca -l 9 | WinRAR -m3 | WinRAR -m5 |
| --- | --- | --- | --- | --- | --- |
| Los 50,000 archivos pequeños, 410,069,593 bytes | 251,139,697 | 247,824,582 | **247,469,613** | 251,926,583 | 251,891,469 |
| Este repositorio en `4b1d67b`, 973 archivos, 16,551,105 bytes | 7,939,633 | 7,826,524 | **7,820,343** | 8,145,493 | 8,133,072 |
| 16 archivos de texto grandes, 326,882,067 bytes | 123,443,446 | 117,071,830 | **117,038,415** | 123,301,735 | 122,832,147 |

```sh
cargo run --release -p arca-bench --example small_files -- C:/b/small
cargo run --release -p arca-bench --example small_files -- C:/b/text 16 33554432 text
mkdir C:/b/src && git archive 4b1d67b | tar -x -C C:/b/src
arca create C:/b/out.zip C:/b/small -l 6
```

```powershell
Start-Process -Wait "C:\Program Files\WinRAR\WinRAR.exe" `
  -ArgumentList "a -afzip -m5 -ibck -y -r -ep1 C:\b\out.zip C:\b\small\*"
```

Lo que cuesta cada nivel, en la misma máquina y con la misma carga, 3 pasadas de cada uno, 16 hilos:

| Nivel | Nivel de libdeflate | Texto grande: reloj, CPU | Archivos pequeños: reloj, CPU |
| --- | --- | --- | --- |
| 6 (por defecto) | 6 | 1.493 s, 8.13 s | 1.325 s, 12.09 s |
| 8 | 10 | 4.964 s, 38.86 s | 3.307 s, 29.83 s |
| 9 (máximo) | 12 | 7.648 s, 58.86 s | 4.494 s, 45.77 s |

```powershell
powershell -File benchmarks\time.ps1 -Exes arca.exe `
  -ArgLine "create C:\b\out.zip C:\b\text -l 8" -Runs 3 -Clean C:\b\out.zip
```

El nivel por defecto es más pequeño que el mejor de WinRAR con archivos pequeños y con código fuente, y un 0.5% más grande con texto grande. El nivel 8 es el primero de los analizadores casi óptimos de libdeflate: un 4.7% más pequeño que el mejor de WinRAR con texto grande y un 1.6% con archivos pequeños, por entre dos veces y media y cinco veces la CPU del nivel por defecto. El nivel 9 apenas mejora al 8. La correspondencia entre los niveles de Arca y los de libdeflate no cambia. Con el corpus de 16 archivos de la tabla anterior, 7-Zip -mx5 quedó un 3.5% por debajo del nivel por defecto de Arca; el nivel 8 queda un 5.2% por debajo del nivel por defecto con el texto grande de aquí, así que debería cerrar esa diferencia, pero eso no se ha medido contra 7-Zip.


## Extracción en paralelo

Windows 11, 16 hilos, 287 MB en 16 archivos de texto, mejor de 3, borrando la salida antes de cada pasada.

| Herramienta | Tiempo |
| --- | --- |
| **Arca, 16 hilos** | **0.207 s** |
| Arca, -j 1 | 0.686 s |
| 7-Zip | 1.020 s |

```sh
arca create big.zip big
arca extract big.zip -o out -j 1
arca extract big.zip -o out
7z x -o"out" big7z.zip
```

## LZMA y LZMA2: .xz, .tar.xz y .7z

Windows 11, Ryzen 7 3700X (8 núcleos, 16 hilos), frente a xz 5.8.1 de Git for Windows; 7-Zip no estaba instalado. Otras compilaciones compartían la máquina mientras se medía, así que cada fila es la mediana de pasadas intercaladas, con su intervalo entre corchetes, y se da tanto el tiempo de reloj como el tiempo de CPU del propio proceso. «Antes» es Arca en el commit `4b1d67b` y «después», el mismo código con el trabajo de LZMA de `roadmap/lzma-speed`; los dos dicen `arca 0.10.0`.

El corpus son 80 MiB que cualquier máquina regenera byte a byte: 24 MiB de prosa, 24 de código parecido a Rust y 24 de código máquina parecido a x86, y 8 MiB de bytes aleatorios.

```sh
cargo build --release
cargo run --release -p arca-bench --example lzma_corpus -- corpus
tar -cf corpus.tar corpus
xz -1 -T0 -kc corpus.tar > t1.tar.xz   # 27 bloques
xz -6 -T0 -kc corpus.tar > t6.tar.xz   # 4 bloques
xz -6 -T1 -kc corpus.tar > s6.tar.xz   # 1 bloque
```

### Extraer un .tar.xz

Tiempo de reloj con su intervalo y, después, tiempo de CPU, mediana de 5. Arca además lee el TAR y escribe los cuatro archivos; xz escribe el `.tar`.

| Archivo | Arca antes | Arca después | xz -T1 | xz -T0 |
| --- | --- | --- | --- | --- |
| `t1.tar.xz`, 27 bloques | 1.70 s [1.44–14.11] · 1.56 s | 0.27 s [0.18–0.89] · 1.64 s | 1.08 s [0.88–1.12] · 1.06 s | **0.20 s** [0.17–0.22] · 1.11 s |
| `t6.tar.xz`, 4 bloques | 1.38 s [1.28–2.96] · 1.34 s | 0.59 s [0.57–0.64] · 1.23 s | 0.87 s [0.81–0.99] · 0.88 s | **0.42 s** [0.40–0.47] · 0.89 s |
| `s6.tar.xz`, 1 bloque | 1.32 s [1.29–1.53] · 1.28 s | 1.13 s [1.11–1.20] · 1.13 s | **0.86 s** [0.83–0.90] · 0.81 s | 0.86 s [0.85–1.00] · 0.86 s |

```sh
arca extract t1.tar.xz -o out
xz -dkf -T1 t1.tar.xz
xz -dkf -T0 t1.tar.xz
```

Un `.xz` escrito en varios hilos es una serie de bloques independientes, y Arca descomprime ahora un bloque por núcleo, como hace `xz -T0`. Un archivo de un solo bloque se sigue descomprimiendo en un hilo; ahí las sumas de comprobación de XZ más rápidas han recortado el tiempo de Arca un 14%, pero sigue gastando 1.13 s de CPU frente a los 0.81 s de xz, en parte por leer el TAR y escribir los archivos: su descompresor, en Rust seguro, es más lento que el de liblzma. Con muchos bloques pequeños Arca se acerca más a `xz -T0`; con cuatro, el bloque mayor marca el ritmo de las dos herramientas.

### Crear un .tar.xz

Mediana de 3. Arca con `-l 1` y `-l 6` frente a `xz -1` y `xz -6` sobre los mismos archivos; los tamaños no se separan más de un 0.03% en ningún nivel.

| Orden | Reloj | CPU | Tamaño |
| --- | --- | --- | --- |
| Arca antes, `-l 1`, 16 hilos | 1.63 s [1.51–1.80] | 11.09 s | 36,000,336 |
| Arca después, `-l 1`, 16 hilos | 1.59 s [1.49–1.62] | 10.98 s | 36,000,336 |
| `xz -1 -T0` | **1.44 s** [1.43–1.55] | 12.13 s | 36,001,028 |
| Arca antes, `-l 6`, 14 hilos | 32.46 s [29.64–36.25] | 86.91 s | 30,456,380 |
| Arca después, `-l 6`, 14 hilos | 38.66 s [26.36–48.31] | 88.78 s | 30,456,380 |
| `xz -6 -T0` | **32.60 s** [24.94–44.92] | 69.53 s | 30,463,552 |
| Arca después, `-l 6 -j 1` | 91.68 s [86.20–91.96] | 76.22 s | 30,341,456 |
| `xz -6 -T1` | **69.56 s** [65.70–149.00] | 65.05 s | 30,347,164 |

```sh
arca create out.tar.xz corpus -l 6
arca create out.tar.xz corpus -l 6 -j 1
xz -6 -T0 -kf corpus.tar
xz -6 -T1 -kf corpus.tar
```

El compresor no ha cambiado: las dos filas de Arca de cada nivel no se separan más que sus intervalos, y en un hilo Arca gasta un 17% más de CPU que xz con el nivel 6. Con ese nivel, la mayor parte del tiempo se va en el buscador de coincidencias en árbol binario, que recorre memoria que la caché no tiene, el mismo trabajo que hace xz. Los dos cortan la entrada en bloques de tres diccionarios, 24 MiB con el nivel 6, así que 80 MiB dan solo cuatro bloques que comprimir a la vez, y el de prosa, el más lento, marca el tiempo de reloj.

### 7z, nivel a nivel

Mediana de 3 para crear y de 5 para extraer, con archivos hechos por Arca a partir del mismo directorio `corpus`. Los tamaños no han cambiado ni un byte.

| Nivel | Crear antes | Crear después | Extraer antes | Extraer después | Tamaño |
| --- | --- | --- | --- | --- | --- |
| 1 | 1.44 s [1.38–1.48] | 1.57 s [1.56–1.62] | 1.52 s [1.29–7.80] | **0.29 s** [0.23–0.58] | 35,998,493 |
| 6 | 32.30 s [31.21–32.97] | 33.83 s [30.69–35.30] | 2.49 s [1.59–15.27] | **0.76 s** [0.59–1.24] | 30,457,591 |
| 9 | 127.48 s [114.34–169.06] | 128.72 s [117.23–173.85] | 1.72 s [1.38–2.06] | 1.59 s [1.24–8.76] | 30,231,863 |

```sh
arca create out.7z corpus -l 6
arca extract out.7z -o out
```

La extracción descomprime ahora los datos LZMA2 en varios hilos allí donde el flujo reinicia su diccionario, como hacen cada pocos diccionarios los compresores multihilo; un bloque de una sola tirada, como el que escribe el nivel 9, se sigue descomprimiendo en un hilo. El tiempo de CPU de la extracción se quedó entre 1.2 y 1.6 s antes y después. Crear con el nivel 1 perdió un 9% de tiempo de reloj, más de lo que separan los intervalos, probablemente porque el compresor LZMA2 multihilo de lzma-rust2 0.22.0, que .7z comparte ahora con .xz, no deja en cola más de un trozo por encima de sus hilos, mientras que 0.21.0 leía por adelantado sin límite.

## Contra NanaZip, en un archivo lo bastante grande como para doler

Un `.zip` de 3.13 GB con 1513 archivos que al descomprimirse ocupan 6.28 GB, todo en Deflate, con 13 entradas de más de 100 MB. Windows 11, 16 núcleos, NanaZip 7.0.1832, y un SSD SATA sin DRAM (Kingston A400).

### Solo descompresión

Ambas herramientas leen cada entrada, comprueban cada CRC y no escriben nada, en un núcleo cada una.

| Herramienta | Reloj | CPU | Núcleos |
| --- | --- | --- | --- |
| **Arca** | **13.2 s** | 13.2 s | 1.0 |
| NanaZip | 25.9 s | 25.9 s | 1.0 |

```sh
arca test big.zip
NanaZipC t big.zip
```

### El trabajo completo

Escribiendo los 6.28 GB enteros, ejecutado en el orden A B B A para que el declive del disco no favorezca a ningún lado.

| Orden | Herramienta | Tiempo |
| --- | --- | --- |
| 1.º | **Arca** | **21.9 s** |
| 2.º | NanaZip | 49.0 s |
| 3.º | NanaZip | 68.0 s |
| 4.º | **Arca** | **28.0 s** |

```sh
arca extract big.zip -o out
NanaZipC x -y -o"out" big.zip
```

El disco sostenía 327 MB/s antes de las cuatro ejecuciones y 90 MB/s después. Arca corrió tanto en el disco más fresco como en el más desgastado, y su peor ejecución aún superó a la mejor de NanaZip.

> [!NOTE]
> **Por qué Arca gana aquí**
>
> El tiempo de reloj de NanaZip es igual a su tiempo de CPU: su propio procesador es el límite. Arca gasta algo más de CPU que de reloj (1.1 núcleos), así que la descompresión se solapa con la escritura y el tiempo de reloj se ajusta al disco: 6.28 GB en 21.9 s son 294 MB/s frente a un techo de 327 MB/s. El decodificador más rápido es lo que lo consigue.

## RAR 5 contra WinRAR

Arca escribe RAR 5 con su propio compresor, una copia parcheada de `rars` (véase `arca-rar/vendor/PATCHES.md`). Estas cifras lo comparan con el `Rar.exe` de consola de WinRAR 7.23 en el nivel al que corresponde cada nivel de Arca: `-l 1`, `3`, `6`, `7` y `9` de Arca contra `-m1` a `-m5`.

AMD Ryzen 7 3700X (8 núcleos, 16 hilos), 32 GB, Windows 11. **La máquina estaba ocupada**: durante todas las pruebas se compilaban otros proyectos, y la carga del procesador medida antes de cada ejecución estuvo casi siempre entre el 60 y el 100%. El tiempo real varía el doble o más de una ejecución a otra, así que las tablas dan primero el tiempo de CPU de todo el proceso (con todos sus hilos) y el tamaño, que la carga mueve mucho menos. Cada cifra es la mediana de 3 ejecuciones; el CSV que escribe el script guarda cada ejecución con su carga.

El corpus se genera, así que cualquiera puede reconstruir los mismos bytes, y como datos reales se añade este repositorio en un commit fijo:

```powershell
cargo build --release -p arca-cli
cargo run --release -p arca-rar --example rar-corpus -- $env:TEMP\rar-corpus
mkdir $env:TEMP\rar-corpus\repo
git archive --format=tar 4b1d67b | tar -x -C $env:TEMP\rar-corpus\repo
powershell -ExecutionPolicy Bypass -File arca-rar\tests\rar-bench.ps1 `
    -Corpus $env:TEMP\rar-corpus -Runs 3 `
    -Kinds "text,log,source,exe,media,random,small,repo,all"
```

Cada ejecución es `arca create out.rar <dir> -l N` o `Rar.exe a -mN -ep1 -r -idq out.rar <dir>`, las dos sin modo sólido, y cada archivo que escribe Arca se comprueba con `Rar.exe t`. «Antes» es Arca en el commit `4b1d67b`, medido de la misma forma el mismo día.

### El corpus entero

`all` es el árbol entero en un solo archivo: los 77 MiB que escribe el generador más los 16 MiB del tipo `repo`, 97,441,208 bytes en 4286 archivos.

| Nivel | Antes: CPU | real | tamaño | **Ahora: CPU** | real | tamaño | WinRAR: CPU | real | tamaño |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `-l 1` / `-m1` | 33.8 s | 123.1 s | 46,836,620 | **21.3 s** | 36.8 s | 46,936,551 | 8.1 s | 2.9 s | 47,234,842 |
| `-l 3` / `-m2` | 81.4 s | 104.6 s | 44,067,047 | **62.2 s** | 32.5 s | 43,880,430 | 12.5 s | 2.9 s | 44,341,281 |
| `-l 6` / `-m3` | 96.7 s | 46.9 s | 43,648,504 | **89.8 s** | 51.9 s | 43,419,805 | 21.2 s | 5.7 s | 42,537,059 |
| `-l 7` / `-m4` | 188.2 s | 135.7 s | 43,332,800 | **79.5 s** | 44.3 s | 43,353,431 | 29.9 s | 6.5 s | 42,258,443 |
| `-l 9` / `-m5` | 181.5 s | 201.7 s | 43,244,674 | **101.7 s** | 82.0 s | 43,240,314 | 31.7 s | 6.2 s | 42,206,709 |

Aquí el tiempo real de Arca no es sobre todo el del compresor: el escritor guarda cada miembro en su propio archivo temporal antes de montar el archivo, y en Windows eso cuesta milisegundos por archivo (véase más abajo). Para comparar compresores, la cifra que vale es el tiempo de CPU. Frente a WinRAR, Arca usa ahora 2.6× su tiempo de CPU con `-l 1`, 4.2× con `-l 6` y 3.2× con `-l 9`, frente a 4.9×, 5.3× y 4.9× antes. Sus archivos salen un 0.6% y un 1.0% más pequeños con `-l 1` y `-l 3`, y salían entre un 2.1% y un 2.6% más grandes de `-l 6` en adelante, casi todo por el tipo `media`, donde WinRAR aplicaba un filtro delta y Arca ninguno; con los filtros de datos de más abajo salen entre un 7.4% y un 7.9% más pequeños.

### Por tipo, `-l 6` contra `-m3`

| Tipo | Antes: CPU | tamaño | **Ahora: CPU** | tamaño | WinRAR: CPU | tamaño |
| --- | --- | --- | --- | --- | --- | --- |
| `text` | 12.48 s | 3,106,714 | **12.45 s** | 3,106,714 | 2.09 s | 3,303,767 |
| `log` | 32.89 s | 3,311,320 | **21.12 s** | 3,042,982 | 5.95 s | 3,524,354 |
| `source` | 3.78 s | 2,125,845 | **4.11 s** | 2,125,845 | 1.11 s | 2,188,815 |
| `exe` | 15.45 s | 7,255,811 | **12.02 s** | 7,255,835 | 3.64 s | 7,711,395 |
| `media` | 8.80 s | 7,985,548 | **1.59 s** | 8,023,086 | 2.30 s | 6,056,962 |
| `random` | 7.62 s | 8,388,863 | **1.11 s** | 8,388,863 | 1.58 s | 8,388,944 |
| `small` | 8.91 s | 3,668,621 | **7.81 s** | 3,668,621 | 1.31 s | 3,549,840 |
| `repo` | 10.30 s | 7,774,219 | **6.84 s** | 7,776,296 | 2.25 s | 7,778,575 |

Con texto, registros y código fuente Arca escribe entre un 3% y un 14% menos que WinRAR con entre tres y seis veces su tiempo de CPU; con el tipo real `repo` los dos quedan a menos de un 0.1% con tres veces. `media` y `random` son ahora más rápidos que en WinRAR, porque la búsqueda salta la entrada que no deja de fallar.

### Filtros de datos

A partir de `-l 5` Arca aplica los filtros E8, E8E9, ARM y delta de RAR 5, elegidos para cada tramo de 64 KiB de un archivo, como hace WinRAR desde `-m3` (véase `arca-rar/vendor/PATCHES.md`). Las tablas anteriores son de antes de los filtros. Se midió de la misma forma, `-Runs 3`, con `-Levels "3,4,5" -Kinds "exe,media,all"`; «Antes» es Arca en el commit `17769b4`, el último sin filtros, ejecutado el mismo día. La máquina volvía a estar ocupada (carga del 40 al 99%), así que los tiempos de CPU son aproximados; los tamaños son exactos.

| Tipo, nivel | Antes: CPU | tamaño | **Filtros: CPU** | tamaño | WinRAR: CPU | tamaño |
| --- | --- | --- | --- | --- | --- | --- |
| `all`, `-l 6` / `-m3` | 88.1 s | 43,419,805 | **85.1 s** | 39,186,348 | 23.1 s | 42,537,059 |
| `all`, `-l 7` / `-m4` | 87.9 s | 43,353,431 | **81.1 s** | 39,145,842 | 35.2 s | 42,258,443 |
| `all`, `-l 9` / `-m5` | 99.9 s | 43,240,314 | **112.4 s** | 39,039,838 | 35.6 s | 42,206,709 |
| `media`, `-l 6` / `-m3` | 2.23 s | 8,023,086 | **8.14 s** | 4,172,707 | 2.28 s | 6,056,962 |
| `media`, `-l 9` / `-m5` | 2.63 s | 7,995,463 | **9.75 s** | 4,159,438 | 2.45 s | 6,052,582 |
| `exe`, `-l 6` / `-m3` | 18.84 s | 7,255,835 | **15.20 s** | 6,874,309 | 4.23 s | 7,711,395 |
| `exe`, `-l 9` / `-m5` | 18.14 s | 7,227,594 | **17.77 s** | 6,864,731 | 8.08 s | 7,647,659 |
| `arca.exe`, `-l 6` / `-m3` | 5.03 s | 2,776,474 | **4.89 s** | 2,630,061 | 1.63 s | 2,625,050 |
| `arca.exe`, `-l 9` / `-m5` | 6.39 s | 2,718,248 | **7.27 s** | 2,569,717 | 2.70 s | 2,612,567 |

El corpus entero sale ahora un 7.9%, un 7.4% y un 7.5% más pequeño que con WinRAR con `-l 6`, `-l 7` y `-l 9`, donde antes salía un 2.1%, un 2.6% y un 2.4% más grande. `media` queda un tercio por debajo de WinRAR y cuesta entre tres y cuatro veces la CPU de antes, porque el audio y los píxeles filtrados merecen la búsqueda que antes se saltaba con los bytes en bruto, así que `media` ya no es más rápido que en WinRAR. Los datos aleatorios sí lo siguen siendo: decidir que un tramo no lleva filtro es una pasada por sus bytes en el hilo que lee, y con el tipo `random` y `-l 6` Arca tardó entre 0.91 y 1.05 s de CPU, igual antes que después. `exe` gana un 5%; los propios filtros de WinRAR hacen ese código sintético más grande que su salida con `-mc-`. El texto, los registros, el código fuente y los datos aleatorios no se tocan y salen byte a byte iguales; `repo` sale 1,449 bytes más pequeño (7,774,847) y `small` 103 bytes más pequeño (3,668,518), por unos pocos archivos binarios que sí se filtran.

`arca.exe` es un ejecutable real: la compilación de release de 7,475,200 bytes del propio `17769b4` (`cargo build --release -p arca-cli`, SHA-256 `a1f29ea94bc3ac117c89df4634bb083941f68148a6212e3cb70842be5c82cc09`), solo en un directorio y medido con `-Kinds arcaexe -Levels "3,5"`. Otra compilación del mismo commit da otros bytes, así que las cifras valen para ese archivo. Arca queda un 0.2% por encima de WinRAR con `-l 6` y un 1.6% por debajo con `-l 9`.

### Dónde sigue perdiendo Arca

- **Tiempo de CPU.** El análisis óptimo busca en un árbol binario en cada posición y pone precio a cada longitud de coincidencia. Comprime más que WinRAR texto, registros y código fuente, pero de `-l 3` en adelante sigue costando entre tres y seis veces el tiempo de CPU de WinRAR.
- **Filtros que WinRAR aplica y Arca no.** Los archivos sólidos y los ficheros de más de 64 MiB se escriben sin filtros de datos, que WinRAR aplica en los dos casos; además, los filtros ven cada tramo de 64 KiB entero, mientras que WinRAR ciñe un rango delta a unos pocos cientos de bytes dentro de uno.
- **Muchos archivos pequeños.** El escritor guarda los bytes comprimidos de cada miembro en un archivo temporal junto a la salida antes de montar el archivo. En Windows eso cuesta milisegundos por archivo, así que los 3000 archivos del tipo `small` tardan segundos de tiempo real con mucho menos tiempo de CPU. Es cosa de ese almacenamiento intermedio del escritor, no del compresor: `arca create out.rar small -l 0` tarda 2.6 s con los mismos archivos.
- **Cabeceras.** Arca guarda un hash BLAKE2sp de cada archivo donde WinRAR guarda un CRC32: 28 bytes más por archivo, unos 84 KB en el tipo `small`.
- **Pocos archivos grandes.** Un miembro se comprime en tramos de un diccionario cada uno (de 1 a 16 MiB según el nivel), y cada tramo es un trabajo, así que un único archivo de 4 MiB con `-l 6` ocupa un solo núcleo.
</content>
