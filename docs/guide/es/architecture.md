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
| `arca-cli` | El binario arca | permitido, sin usar |
| `arca-gui` | La ventana arca-gui | prohibido |
| `arca-icons` | El icono que el escritorio muestra para un tipo de archivo | solo Windows, para la llamada al shell |
| `windows/arca-shell` | Menú contextual del Explorador, fuera del workspace para que cargo build siga funcionando en Linux y macOS | necesario: COM |

El workspace también contiene `arca-drag` y `arca-net`; consulta [sus fuentes](https://github.com/beyondhumane/arca) para más detalles.

## La política de unsafe

Todo crate que analiza bytes de un archivo declara `#![forbid(unsafe_code)]` a nivel de crate, de modo que el compilador garantiza que no hay código unsafe en ellos. Solo aparece donde el sistema operativo lo exige: una llamada al shell para los iconos en Windows, y COM para la extensión del Explorador.

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
| `rayon` | El pool de hilos detrás de la compresión y extracción en paralelo |
| `clap` | Análisis de línea de comandos |
| GPUI | El framework de UI acelerado por GPU detrás de la ventana |

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
