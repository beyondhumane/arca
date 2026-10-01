---
description: Cada medición, la máquina detrás y los casos donde Arca pierde.
group: Referencia
order: 14
keywords: rendimiento velocidad benchmarks nanazip 7-zip zip números performance speed benchmarks nanazip 7-zip zip numbers
---

# Benchmarks

Estos números vienen de máquinas concretas y no deben leerse como una ventaja general. Cada uno trae su método, incluidos los casos donde Arca pierde.

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
