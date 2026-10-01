---
description: Mira dentro de los archivos y extráelos en paralelo y con seguridad.
group: Uso de Arca
order: 5
keywords: listar extraer descomprimir paralelo conflicto sobrescribir omitir renombrar zip slip -o list extract unpack parallel conflict overwrite skip rename zip slip -o
---

# Listar y extraer

## Listar

```text
arca list <ARCHIVE> [-t | --time]
```

Imprime una línea por entrada (tamaño sin comprimir, método, ratio y nombre) sin extraer nada.

```sh
arca list arca.zip --time
```

```text
       22966  deflate   72.6%  arca/arca-cli/src/main.rs
      114710  deflate   76.8%  arca/arca-zip/src/lib.rs
174 entries, 2.3 MB uncompressed, listed in 0.1 ms
```

Con `--time`, el resumen va a la salida de error estándar, así que canalizar el listado hacia otras herramientas queda limpio.

## Extraer

```text
arca extract <ARCHIVE> [-o DIR] [--on-conflict POLICY] [-j THREADS] [-p PASSWORD]
```

| Opción | Por defecto | Descripción |
| --- | --- | --- |
| `-o, --dest` | `.` | Directorio de destino. Se crea si no existe. |
| `--on-conflict` | overwrite | Qué hacer cuando un archivo ya existe: overwrite, skip o rename. |
| `-j, --threads` | 0 | Hilos a usar. 0 significa todos los núcleos. Solo .zip puede ir en paralelo. |
| `-p, --password` | — | Contraseña de un archivo cifrado (AES-256 o ZipCrypto). |

### Conflictos

| Política | Comportamiento |
| --- | --- |
| `overwrite` | Sustituye el archivo que ya está ahí. |
| `skip` | Deja intacto el archivo existente y continúa. |
| `rename` | Lo escribe junto al otro con el nombre name (1).ext. |

Los destinos se deciden de antemano, en un solo hilo, antes de escribir nada, así que dos entradas con el mismo nombre nunca pueden competir por el mismo nombre libre.

## Extracción en paralelo

Un `.zip` es de acceso aleatorio: el directorio central dice dónde empieza cada entrada, así que un hilo por núcleo puede abrir el archivo cada uno y descomprimir una entrada distinta. Un `.tar` es un único flujo, y un `.tar.gz` un único flujo gzip encima de este, así que no hay nada que dividir y la extracción sigue siendo secuencial.

| 287 MB en 16 archivos de texto · Windows 11 | Tiempo |
| --- | --- |
| Arca, 16 hilos | **0.207 s** |
| Arca, -j 1 | 0.686 s |
| 7-Zip | 1.020 s |

> [!NOTE]
> **Muchos archivos pequeños**
>
> Dividir no cambia nada cuando hay miles de archivos diminutos: extraer 5,358 archivos fuente tardó 4.5 s, pero descomprimir esos mismos 55 MB tardó 0.128 s. El 97% del tiempo se va en crear archivos en NTFS, y 7-Zip tardó 4.6 s con los mismos archivos.

## Rutas seguras

Cada nombre de entrada pasa por la defensa Zip Slip de Arca antes de unirse al destino. Una entrada como `../../etc/passwd` se rechaza con un error en lugar de escribirse fuera del directorio de destino.
