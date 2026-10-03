---
description: Mira dentro de los archivos y extráelos en paralelo y con seguridad.
group: Uso de Arca
order: 5
keywords: listar extraer descomprimir paralelo conflicto sobrescribir omitir renombrar zip slip -o list extract unpack parallel conflict overwrite skip rename zip slip -o
---

# Listar y extraer

## Listar

```text
arca list <ARCHIVE> [-t | --time] [-p PASSWORD]
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
| `-p, --password` | ninguna | Contraseña de un archivo cifrado (AES-256 o ZipCrypto). |

### Conflictos

| Política | Comportamiento |
| --- | --- |
| `overwrite` | Sustituye el archivo que ya está ahí. |
| `skip` | Deja intacto el archivo existente y continúa. |
| `rename` | Lo escribe junto al otro con el nombre name (1).ext. |

Los destinos ZIP se deciden de antemano en un hilo. 7z resuelve los conflictos secuencialmente al recorrer los bloques; los nombres duplicados no compiten por el mismo nombre libre.

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

## 7z, contraseñas y bloques sólidos

```sh
arca list secret.7z -p "a password"
arca test secret.7z -p "a password"
arca extract secret.7z -o restored/ -p "a password" --on-conflict rename
```

Se puede listar un 7z sin cifrar o con cabeceras visibles sin contraseña.
Las cabeceras ocultas la exigen incluso para listar. El listado no verifica el
contenido: usa `test`. Solo el primer miembro de un bloque sólido muestra su
tamaño comprimido; los ratios por fichero no son medidas independientes.

La extracción 7z es secuencial, también para selecciones en la ventana. Hay que
descomprimir los datos sólidos omitidos. Para cualquier 7z cifrado, Arca valida
todo el contenido antes de crear o sustituir ficheros o directorios de destino,
y extrae lo seleccionado en una segunda pasada. Una contraseña incorrecta o un
fallo de checksum cifrado deja el destino intacto. CRC no es autenticación: también
puede indicar corrupción, y el origen debe permanecer estable entre pasadas.

Se rechazan enlaces simbólicos y puntos de reanálisis en el destino. Las
sustituciones usan temporales, sin truncar enlaces duros existentes. Esto no
elimina las carreras ante cambios concurrentes del sistema de archivos. La
extracción sin cifrar puede fallar tras escribir entradas previas; no hay una
reversión de todo el directorio. Consulta los límites de recursos y códecs en
[Arquitectura](architecture.md).

## Rutas seguras

Cada nombre de entrada pasa por la defensa Zip Slip de Arca antes de unirse al destino. Una entrada como `../../etc/passwd` se rechaza con un error en lugar de escribirse fuera del directorio de destino.
