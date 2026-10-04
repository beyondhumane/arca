---
description: Los crates que forman Arca y la política de unsafe que los mantiene unidos.
group: Referencia
order: 12
keywords: crates workspace unsafe diseño dependencias perfiles lto crates workspace unsafe design dependencies profiles lto
---

# Arquitectura

Arca es un workspace de Cargo. Cada crate tiene una sola tarea y una postura declarada sobre el código unsafe.

## Crates

| Crate | Qué hace | unsafe |
| --- | --- | --- |
| `arca-core` | Errores, límites, lecturas de cabecera acotadas, fechas MS-DOS, defensa contra Zip Slip | prohibido |
| `arca-zip` | ZIP con Zip64; store, Deflate sobre zlib-rs, Zstandard, AES-256 | prohibido |
| `arca-tar` | TAR ustar con verificación de suma de comprobación | prohibido |
| `arca-7z` | Análisis 7z acotado, extracción sólida, creación Store/LZMA2 y AES | prohibido |
| `arca-rar` | Adaptador RAR/CBR experimental, opcional y de solo lectura | prohibido |
| `arca-cli` | El binario arca | permitido, sin usar |
| `arca-gui` | La ventana arca-gui | prohibido |
| `arca-icons` | El icono que el escritorio muestra para un tipo de archivo | solo Windows, para la llamada al shell |
| `windows/arca-shell` | Menú contextual del Explorador, fuera del workspace para que cargo build siga funcionando en Linux y macOS | necesario: COM |

El workspace también contiene `arca-drag` y `arca-net`; consulta [sus fuentes](https://github.com/beyondhumane/arca) para más detalles.

CLI y escritorio comparten `arca_core::Format`. Los formatos de escritura
incluyen 7z, pero no RAR. El [lector RAR](../rar.md) se activa solo con
`--features rar`; `rars` habilita el cifrado, nunca su escritor. El escritorio
comparte la validación de contraseñas en segundo plano entre ZIP, 7z y RAR, con
reintentos, cancelación y reanudación de la acción original. El CRC es opcional:
una suma ausente no se muestra como un cero inventado.

## La política de unsafe

Los crates de análisis de Arca declaran `#![forbid(unsafe_code)]`; esto no abarca
todas las dependencias transitivas. Bzip2/Zstandard nativos son códecs opcionales;
las bibliotecas gráficas y del SO tienen sus propios límites de unsafe. El parser
7z vendorizado y su decodificador LZMA fijado compilan con unsafe prohibido.

```rust
#![forbid(unsafe_code)]

// A malformed archive produces an error, never memory corruption.
match ZipArchive::open(file) {
    Ok(archive) => list(archive),
    Err(e) => eprintln!("arca: {e}"),
}
```

## Dependencias clave

| Crate | Para qué se usa |
| --- | --- |
| `flate2` + `zlib-rs` | Deflate, usando la implementación en Rust más rápida medida durante el diseño |
| `zstd` | Bindings de Zstandard a libzstd, con su multihilo interno activado |
| `crc32fast` | Comprobaciones CRC-32 |
| `sevenz-rust2` 0.23.0 / `lzma-rust2` 0.21.0 | 7z/AES y LZMA Apache-2.0; parser reforzado, optimización unsafe de LZMA desactivada |
| `rayon` | El pool de hilos detrás de la compresión y extracción en paralelo |
| `clap` | Análisis de línea de comandos |
| GPUI | El framework de UI acelerado por GPU detrás de la ventana |

## Límites de 7z

- Cabeceras codificadas/descodificadas: 16 MiB cada una; entradas, bloques y flujos: 100.000.
- Hasta cuatro codificadores de una entrada/salida por bloque; BCJ2 multientrada no se admite.
- Diccionarios LZMA: 256 MiB por etapa; ventana Zstd: 256 MiB. No es una cuota
  global de memoria. No hay límite total de bytes extraídos ni de tiempo de CPU.
- Lectura básica: Copy, LZMA, LZMA2, DEFLATE, AES, BCJ/Delta de un flujo.
  `codecs-native` añade Bzip2 y Zstd. PPMd, Brotli y LZ4 no se admiten.
- La extracción cifrada valida todo el origen estable antes de escribir y luego
  procesa los bloques seleccionados. CRC-32 no es autenticación.
- La cancelación es cooperativa; el análisis de cabeceras y la KDF están acotados
  pero no se pueden interrumpir internamente. La vista previa limita el tamaño.
- `Entry.offset` es un índice en 7z, no un desplazamiento. `Method::code()` devuelve
  ahora `Result<u16>` y rechaza métodos que no sean ZIP.

CLI/GUI propagan `codecs-native` explícitamente, sin activar los valores por
defecto de las dependencias de archivos. `cargo build --release --no-default-features`
omite compresión nativa, no bibliotecas del SO/gráficas. Para auditarlo:

```sh
cargo tree -p arca-cli --no-default-features -e normal
cargo tree -p arca-gui --no-default-features -e normal
cargo tree -p arca-7z -e features -i lzma-rust2
```

Los dos primeros no deben contener los nativos `zstd-sys` ni `bzip2-sys`.
La GUI incluye `libbz2-rs-sys` por la descompresión HTTP de GPUI: pese al nombre,
es una implementación Rust sin compilación C, no el decodificador Bzip2 de 7z.
LZMA debe activar `std`/`encoder`, nunca `optimization`. El workspace mantiene
Rust 1.95 y edición 2021; cada dependencia elige su edición. Consulta el
[diseño aceptado](https://github.com/beyondhumane/arca/blob/main/docs/plans/7z-format.md)
y el [inventario de parches](https://github.com/beyondhumane/arca/blob/main/arca-7z/vendor/PATCHES.md).

## Perfiles de compilación

```toml
[profile.dev.package."*"]
opt-level = 3

[profile.dev]
opt-level = 1

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

Las dependencias se optimizan incluso en builds de depuración: el motor de maquetación de GPUI, el shaper de texto y el rasterizador rehacen su trabajo en cada fotograma, y con `opt-level = 0` eso se nota como tirones al arrastrar. El código propio de Arca compila en el nivel 1, lo que mantiene el depurador útil.
