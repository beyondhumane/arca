---
description: Lo que todavía no está, y por qué importa parte de ello.
group: Referencia
order: 15
keywords: hoja de ruta futuro 7z lzma xz sólido enlaces simbólicos planificado roadmap future 7z lzma xz solid symlinks planned
---

# Hoja de ruta

Lo que Arca todavía no hace y, donde importa, por qué.

## Disponible

7z permite listar y extraer (incluidos sólidos y cabeceras cifradas), crear con
Store/LZMA2, contraseñas y nombres ocultos en CLI y escritorio. Consulta el
[issue #2](https://github.com/beyondhumane/arca/issues/2) y el
[diseño aceptado](https://github.com/beyondhumane/arca/blob/main/docs/plans/7z-format.md).

`.xz` suelto y `.tar.xz`/`.txz` se pueden crear, listar, probar y extraer en CLI
y escritorio. Consulta el
[issue #18](https://github.com/beyondhumane/arca/issues/18) y
[Crear archivos](creating-archives.md#xz).

## Todavía no

La [lectura RAR/CBR](../rar.md) es experimental y se activa con `--features rar`.
La activación estable, la revisión de procedencia y los archivos multivolumen
siguen pendientes. No se admite crear ni modificar RAR.

- **Enlaces simbólicos:** se omiten al crear ZIP/TAR y se rechazan en 7z.
- **Nombres largos de GNU tar.**
- **Crear 7z sólidos:** se leen los existentes, pero se crean bloques independientes.
- **Cambiar contraseñas y modificar 7z:** añadir, borrar y renombrar puede exigir reescribir bloques sólidos y queda fuera del alcance.
- **Integración de escritorio en Linux y macOS:** el menú del Explorador es solo para Windows por ahora.
- **Modificar fuera de ZIP:** ZIP ya se edita en la ventana; 7z solo se crea y se lee.
- **Un formato nativo:** momento en el que Zstandard pasa a ser el códec por defecto.

> [!TIP]
> **¿Quieres ayudar?**
>
> Elige un punto, abre un issue para discutir el enfoque y envía un pull request. Consulta [Contribuir](contributing.md).
</content>
