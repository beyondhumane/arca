---
description: Explorar imágenes de disco VHD, VHDX, VMDK, DMG y en bruto en solo lectura, como hace 7-Zip, con particiones MBR y GPT y FAT, exFAT, NTFS, HFS+ e ISO 9660 dentro.
group: Referencia
order: 23
keywords: vhd vhdx vmdk dmg img imagen de disco máquina virtual partición mbr gpt fat exfat ntfs hfs solo lectura read-only
---

# Imágenes de disco

[English](../disk-images.md) | **Español**

Arca abre las imágenes de disco de las máquinas virtuales, de los instaladores
de macOS y de las tarjetas SD y memorias USB, y muestra los archivos que hay
dentro, como hace 7-Zip. Vienen incluidas, no necesitan ninguna feature de
Cargo y son de **solo lectura**: Arca nunca las crea, las modifica ni las
monta. Listar, comprobar, extraer, las vistas previas, las sumas de las
entradas en la ventana y las comparaciones (`arca diff`) funcionan como con
cualquier otro archivo.

```sh
arca list server.vhdx
arca test macOS-installer.dmg
arca extract sdcard.img -o sdcard/
arca diff guest.vmdk restored/
```

## Contenedores

| Formato | Sufijo | Qué se lee |
|---|---|---|
| Imagen en bruto | `.img` | Los bytes del disco tal cual. |
| VHD | `.vhd` | Discos fijos y dinámicos, con las sumas del pie y de la cabecera dinámica comprobadas. Los discos diferenciales se rechazan: sus datos están repartidos con un disco padre. |
| VHDX | `.vhdx` | Discos fijos y dinámicos con sectores de 512 o 4096 bytes. Las cabeceras y la tabla de regiones se comprueban con su CRC-32C, y gana la más reciente de las dos cabeceras. Una imagen cuyo registro aún hay que reproducir, porque no se cerró limpiamente, se rechaza en lugar de leerse mal: conéctala una vez en Windows o Hyper-V y desconéctala. Los discos diferenciales se rechazan. |
| VMDK | `.vmdk` | Extensiones monolíticas dispersas y optimizadas para flujo (granos comprimidos con deflate), y descriptores de texto cuyas extensiones `FLAT`, `ZERO` y `SPARSE` están a su lado, como `monolithicFlat` y los discos partidos `twoGbMaxExtent`. Una extensión nombrada con una carpeta, como `../disk.vmdk`, se rechaza. Las instantáneas (un `parentCID` distinto de `ffffffff`) se rechazan. |
| DMG | `.dmg` | Imágenes UDIF de Apple: el pie `koly`, el mapa de bloques `blkx` de la lista de propiedades y trozos guardados tal cual, rellenos de ceros o comprimidos con zlib, bzip2, ADC o LZMA (xz). Sin nada seleccionado, `arca test` comprueba además el CRC-32 de cada bloque con sus datos descomprimidos, como hace 7-Zip. |

Un VHD dinámico, un VHDX y un VMDK se reconocen también por sus primeros bytes
cuando su nombre no dice nada. Un VHD fijo, un DMG y una imagen en bruto no
guardan nada reconocible al principio, así que solo el sufijo dice qué son.

## Particiones y sistemas de archivos

Un disco con MBR (incluidas las particiones lógicas de una extendida) o con
GPT muestra cada partición como una carpeta numerada en la raíz, con el nombre
de su sistema de archivos: `0.ntfs/`, `1.fat/`, `2.exfat/`, `3.hfs/`, `4.iso/`.
Un DMG lista sus bloques igual, sin el espacio libre. Una partición cuyo
sistema de archivos Arca no lee, como ext4, APFS o un área de intercambio de
Linux, aparece como un único archivo en bruto, `N.img`, que se puede extraer y
abrir por separado. Un disco sin tabla de particiones que contiene directamente
un sistema de archivos, como un disquete o una memoria USB «superfloppy»,
muestra sus archivos en la raíz; uno que no contiene nada que Arca conozca es un
único archivo en bruto con el nombre de la imagen.

| Sistema de archivos | Qué se lee |
|---|---|
| FAT12, FAT16, FAT32 | Nombres largos, los indicadores de minúsculas de NT para los nombres cortos y las fechas de creación, modificación y acceso. |
| exFAT | Archivos contiguos y encadenados, la longitud de datos válidos (el resto se lee como ceros) y la zona horaria de cada fecha. Los grupos de entradas se comprueban con su suma. |
| NTFS | Todos los archivos y carpetas de la tabla maestra de archivos, con datos residentes, dispersos y fragmentados y archivos cuyos registros continúan en registros de extensión. Los flujos de datos alternativos no se muestran. |
| HFS+ y HFSX | El catálogo, el archivo de desbordamiento de extensiones, los enlaces simbólicos y los enlaces duros de archivos. Una `/` en un nombre se muestra como `:`, como hace el Finder. |
| ISO 9660 | Con el mismo lector que los [archivos `.iso`](iso.md), con nombres Rock Ridge y Joliet. |

## Qué no se lee

- **APFS y ext2/3/4** no se exploran: sus particiones aparecen como archivos
  en bruto `N.img`. La mayoría de los DMG actuales de macOS contienen APFS.
- **Trozos LZFSE en un DMG.** El único decodificador LZFSE en Rust puro se
  apoya en cientos de bloques `unsafe`, y Arca no le pasa bytes que no son de
  fiar. El listado lo indica, y las particiones a las que pertenecen esos
  trozos no se pueden leer.
- **Los DMG cifrados** (`encrcdsa`), **los DMG segmentados** y los DMG antiguos
  que guardan su mapa de bloques solo en un *resource fork* se rechazan.
- **Los archivos NTFS comprimidos o cifrados por NTFS**, y sus enlaces
  simbólicos, uniones y demás puntos de reanálisis, se omiten y se cuentan en
  una nota.
- **Los archivos HFS+ comprimidos por HFS+** (el indicador `UF_COMPRESSED`,
  habitual en archivos copiados por el Finder o instalados por macOS) y los
  enlaces duros de carpetas se omiten y se cuentan en una nota.
- **Los discos VHD y VHDX diferenciales y las instantáneas VMDK**, que
  necesitan su disco padre.
- Los volúmenes lógicos (LVM, Espacios de almacenamiento), RAID y los volúmenes
  BitLocker o FileVault aparecen como particiones en bruto.

## Por qué Arca nunca se queda con estos archivos

Las imágenes de disco pertenecen al sistema que las monta: Windows conecta los
discos VHD y VHDX y monta los archivos `.img` con un doble clic, macOS abre los
DMG con su propio montador, y los discos VMDK son de VMware y VirtualBox. Por
eso el instalador nunca registra estos sufijos, ni siquiera a petición; ábrelos
desde la ventana, arrastrándolos a ella o con *Abrir con*.

## Seguridad

Todos los analizadores son de Arca, en Rust seguro, y una imagen dañada u
hostil da un error o una nota, nunca un cierre inesperado:

- ninguna estructura se reserva a partir de un tamaño declarado antes de
  contrastar ese tamaño con la imagen: las tablas de bloques, los directorios
  de granos, la lista de propiedades del DMG (64 MiB como mucho) y cada trozo
  descomprimido (64 MiB como mucho) se comprueban primero;
- las lecturas nunca pasan del final del disco virtual, y las zonas que la
  imagen no guarda se leen como ceros;
- las particiones que empiezan pasado el disco se omiten, y las que se salen
  de él se cortan donde acaba; la cadena de particiones lógicas se sigue como
  mucho 128 registros y se detiene ante un bucle;
- las cadenas de clústeres de FAT y exFAT, los enlaces al padre de NTFS y las
  cadenas de hojas de los árboles B de HFS+ se detienen ante un bucle, las
  carpetas se siguen como mucho 128 niveles, y todos los sistemas de archivos
  comparten un mismo presupuesto de tramos de datos;
- la extracción pasa por las mismas comprobaciones de destino que cualquier
  otro formato, así que un nombre que sale de la carpeta se rechaza.

Las pruebas construyen cada imagen byte a byte (volúmenes FAT, exFAT y HFS+,
tablas MBR y GPT, contenedores VHD, VHDX, VMDK y DMG) y la dañan: cortada a
muchas longitudes, o con bytes cambiados en sus cabeceras, debe abrirse,
comprobarse y leerse sin un pánico. El volumen NTFS es la imagen `testfs1` de
2 MiB del crate [`ntfs`](https://crates.io/crates/ntfs) (MIT o Apache-2.0),
guardada como `crates/arca-disk/tests/fixtures/ntfs.img.xz`.

```sh
cargo test -p arca-disk
cargo test -p arca-cli --test disk
```
