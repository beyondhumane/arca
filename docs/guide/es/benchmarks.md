---
description: Cada medición, la máquina detrás y los casos donde Arca pierde.
group: Referencia
order: 14
keywords: rendimiento velocidad benchmarks nanazip 7-zip zip números puntuación códecs hilos performance speed benchmarks nanazip 7-zip zip numbers rating codecs threads
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

Con archivos grandes, Arca comprimió 5.7× más rápido a costa de un 3.7% más de tamaño; con muchos archivos pequeños, 7-Zip fue algo mejor con el mismo tamaño. El ganador depende del corpus.

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
</content>
