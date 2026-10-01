---
description: Crea, lista, verifica y extrae un archivo en cuatro comandos.
group: Primeros pasos
order: 3
keywords: tutorial primeros pasos alias conceptos básicos tutorial first steps aliases getting started basics
---

# Guía rápida

Cuatro comandos cubren casi todo. Cada uno imprime una única línea de resumen al terminar.

1. **Crear un archivo**

   ```sh
   arca create photos.zip ~/Pictures/trip/
   ```

   El formato viene dado por la extensión. El resumen tiene este aspecto:

   ```text
   photos.zip: 412 files, 1.8 GB -> 1.7 GB (4.1% smaller) in 2.114 s · 871 MB/s · 16 threads
   ```

2. **Mirar dentro**

   ```sh
   arca list photos.zip
   ```

   Una línea por entrada: tamaño, método, ratio y nombre. No se extrae nada.

3. **Comprobarlo**

   ```sh
   arca test photos.zip
   ```

   Se verifica cada CRC, y no se escribe nada en disco.

4. **Extraerlo**

   ```sh
   arca extract photos.zip -o trip/
   ```

   Los archivos `.zip` se extraen en paralelo, una entrada por hilo.

## Alias cortos

| Comando | Alias | Ejemplo |
| --- | --- | --- |
| `create` | `c` | `arca c out.zip src/` |
| `list` | `l` | `arca l out.zip` |
| `extract` | `x` | `arca x out.zip -o dest/` |
| `test` | `t` | `arca t out.zip` |

## Los formatos vienen dados por la extensión

| Extensión | Formato |
| --- | --- |
| `.zip` | ZIP, con Zip64 cuando hace falta |
| `.tar` | TAR ustar |
| `.tar.gz` · `.tgz` | TAR dentro de un flujo gzip |

Cualquier otra extensión se rechaza con un error que enumera las admitidas.

## Obtener ayuda

```sh
arca --help
arca create --help
arca --version
```
