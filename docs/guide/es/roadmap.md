---
description: Lo que todavía no está, y por qué importa parte de ello.
group: Referencia
order: 15
keywords: hoja de ruta futuro 7z lzma xz sólido enlaces simbólicos udf planificado roadmap future 7z lzma xz solid symlinks planned
---

# Hoja de ruta

Lo que Arca todavía no hace y, donde importa, por qué.

## Disponible

7z permite listar y extraer (incluidos sólidos y cabeceras cifradas), crear con
Store/LZMA2, contraseñas y nombres ocultos en CLI y escritorio. Consulta el
[issue #2](https://github.com/beyondhumane/arca/issues/2) y el
[diseño aceptado](https://github.com/beyondhumane/arca/blob/main/docs/plans/7z-format.md).

La [lectura RAR/CBR](rar.md) está activa por defecto, incluidos conjuntos
multivolumen modernos y antiguos, contraseñas, vistas previas y extracción
verificada. Se pueden crear nuevos archivos RAR5 de un solo volumen con cuatro
niveles. Los RAR/CBR existentes siguen siendo de solo lectura: no se admite
modificación, ni salida cifrada, sólida o multivolumen, ni creación de CBR. Un
RAR de un solo volumen creado con registro de recuperación se puede reconstruir
en un archivo nuevo con `arca repair`.

`.xz` suelto y `.tar.xz`/`.txz` se pueden crear, listar, probar y extraer en CLI
y escritorio. Consulta el
[issue #18](https://github.com/beyondhumane/arca/issues/18) y
[Crear archivos](creating-archives.md#xz). bzip2 y Zstandard funcionan igual,
sueltos y alrededor de un TAR; consulta
[Crear archivos](creating-archives.md#bzip2-y-zstandard). Las entradas ZIP
comprimidas con bzip2 se leen.

## Todavía no

- **UDF:** las imágenes ISO se leen por ISO 9660, así que los ficheros que solo están en UDF (como `install.wim` en los medios de Windows) todavía no aparecen.
- **Enlaces simbólicos:** se omiten al crear ZIP/TAR y se rechazan en 7z.
- **Cambiar contraseñas de 7z:** añadir, borrar y renombrar entradas reescribe el archivo con su propia contraseña; cambiar esa contraseña todavía no se ofrece.
- **Integración de escritorio en Linux y macOS:** el menú del Explorador es solo para Windows por ahora.
- **Modificar fuera de ZIP, TAR y 7z:** RAR sigue siendo de solo lectura una vez creado, y las imágenes ISO y los contenedores ZIP nunca se modifican.
- **Un formato nativo:** momento en el que Zstandard pasa a ser el códec por defecto.

> [!TIP]
> **¿Quieres ayudar?**
>
> Elige un punto, abre un issue para discutir el enfoque y envía un pull request. Consulta [Contribuir](contributing.md).
</content>
