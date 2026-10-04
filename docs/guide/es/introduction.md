---
description: Un archivador multiplataforma escrito en Rust seguro, con línea de comandos, ventana nativa y un núcleo compartido.
group: Primeros pasos
order: 1
keywords: resumen qué por qué estado acerca de overview what why status about
---

# Introducción

Arca es un archivador multiplataforma escrito en Rust. Crea y extrae archivos ZIP, 7z y TAR, comprime con Deflate o Zstandard usando todos los núcleos de tu máquina, y cifra con AES-256. Funciona como un único binario de línea de comandos, una ventana de escritorio nativa o el menú contextual del Explorador de Windows.

## ¿Por qué otro archivador?

Los analizadores de archivos comprimidos son una superficie de ataque clásica, y la mayoría de los que se usan a diario están escritos en lenguajes sin seguridad de memoria. Los analizadores de contenedores de Arca están escritos en Rust seguro con `#![forbid(unsafe_code)]` a nivel de crate: un archivo mal formado produce un error, nunca corrupción de memoria.

También es rápido. En la máquina donde se midió, Arca con Zstandard comprimió el corpus Silesia 13.2× más rápido que `zip` y 18.1× más rápido que 7-Zip, en los mismos dos núcleos. Donde no gana, los [benchmarks](benchmarks.md) lo dicen.

## De un vistazo

- **Formatos:** ZIP con Zip64 (Store/Deflate/Zstandard), 7z (creación Store/LZMA2 y lectura sólida), TAR ustar y `.tar.gz`. Las [imágenes ISO 9660](iso.md) se leen, nunca se escriben.
- **Velocidad:** compresión multihilo, extracción paralela de `.zip`, y un arranque en frío por debajo del milisegundo.
- **Seguridad:** analizadores sin código unsafe, lecturas de cabecera acotadas y defensa contra Zip Slip en cada nombre de entrada.
- **Cifrado:** WinZip AE-2 en ZIP; AES-256 en 7z con cabeceras cifradas opcionales. Consulta los [límites de integridad](encryption.md).
- **Interfaces:** la línea de comandos `arca`, la ventana `arca-gui` y un menú del Explorador de Windows 11.
- **Licencia:** Apache-2.0, libre para uso personal y comercial.

## Estado del proyecto

> [!NOTE]
> Arca cubre la fase F01 y parte de la F03 de su diseño (el núcleo, ZIP y TAR, Zstandard, compresión multihilo y línea de comandos), además de 7z con cabeceras cifradas, cifrado AES-256, la ventana de escritorio e integración con el Explorador de Windows. La [hoja de ruta](roadmap.md) indica lo que todavía falta.

## Próximos pasos

- [Instalación](installation.md): descarga una versión o compila desde el código fuente.
- [Guía rápida](quick-start.md): crea, lista, verifica y extrae en cuatro comandos.
- [Referencia de la CLI](cli-reference.md): todos los comandos, argumentos y valores por defecto.
- [Cifrado](encryption.md): cómo funciona AES-256 en Arca, y sus límites.
