---
description: Leer archivos CPIO, AR, DEB, RPM, CAB y LZH/LHA, los otros formatos que abre 7-Zip, sin escribir nunca uno.
group: Referencia
order: 22
keywords: cpio ar deb rpm cab cabinet lzh lha mszip lzx initramfs paquete debian solo lectura read-only
---

# CPIO, AR, DEB, RPM, CAB y LZH

Arca lee seis formatos más de los que abren 7-Zip y NanaZip. Vienen incluidos,
no necesitan ninguna feature de Cargo y son de **solo lectura**: Arca nunca los
crea ni los modifica. Listar, comprobar, extraer, las vistas previas, las sumas
de comprobación de las entradas en la ventana y las comparaciones (`arca diff`)
funcionan como con cualquier otro archivo.

```sh
arca list hello_1.0_amd64.deb
arca test driver.cab
arca extract initrd.cpio -o initrd/
arca diff hello-1.0-1.x86_64.rpm desempaquetado/
```

En la ventana el título dice `CAB: read-only` (o el nombre del formato), y
añadir, borrar, renombrar, mover, nueva carpeta y contraseña se rechazan antes
de tocar el fichero.

| Formato | Sufijos | Qué se lee |
|---|---|---|
| CPIO | `.cpio` | `newc` y `crc` (initramfs de Linux sin comprimir, contenido de los RPM), `odc` de POSIX y las cabeceras binarias antiguas en cualquier orden de bytes. Las sumas de `crc` se comprueban. |
| AR | `.ar`, `.a` | Archivos de Unix y bibliotecas estáticas, con nombres largos de GNU (`//`) y de BSD (`#1/`). Las tablas de símbolos no son ficheros y no se listan. Los archivos «thin» se rechazan. |
| DEB | `.deb`, `.udeb` | Los ficheros que instala el paquete y sus ficheros de control en `DEBIAN/`, como los deja `dpkg-deb -R`. `data.tar` y `control.tar` pueden ir sin comprimir o con gzip, xz, bzip2 o Zstandard. |
| RPM | `.rpm` | El CPIO que lleva dentro, sin comprimir o con gzip, xz, bzip2 o Zstandard. Las firmas y los resúmenes de la cabecera no se comprueban, y el listado lo dice. |
| CAB | `.cab` | Carpetas sin comprimir, MSZIP y LZX, comprobando la suma de cada bloque. |
| LZH | `.lzh`, `.lha` | Cabeceras de nivel 0 a 3; `-lh0-` a `-lh7-`, `-lz4-`, `-lzs-`, `-lz5-`, `-pm0-` a `-pm2-`. Se comprueba el CRC-16 de cada fichero. |

Un fichero cuyo nombre no dice nada se reconoce por sus primeros bytes, y un
`ar` cuyo primer miembro es `debian-binary` se lee como DEB.

### Por qué un DEB enseña sus ficheros y no sus dos tarballs

7-Zip abre un `.deb` como el `ar` que es: `debian-binary`, `control.tar.xz` y
`data.tar.xz`, que hay que abrir uno a uno. Arca enseña lo que hay dentro, el
mismo árbol que desempaqueta `dpkg-deb -R`: los ficheros del paquete donde se
instalan, y `control`, `md5sums` y los scripts del mantenedor en `DEBIAN/`. Un
RPM se enseña igual, como los ficheros de su contenido.

## Enlaces y ficheros especiales

CPIO, DEB y RPM llevan enlaces simbólicos y enlaces duros de Unix. Se tratan
como en TAR: un enlace simbólico solo se crea en Unix y solo si su destino queda
dentro de la carpeta de destino, y un enlace duro se convierte en un enlace al
fichero extraído con él, o en una copia donde el sistema de ficheros no los
tiene. Lo demás se deja fuera y se avisa. Los dispositivos, las tuberías y los
sockets ni se listan ni se extraen; la línea de órdenes escribe
`note: N device files, pipes or sockets are not listed`.

## Qué no se lee

- **CAB:** las carpetas comprimidas con Quantum se listan pero no se extraen, y
  los ficheros que siguen en otro cabinet del conjunto se dejan fuera con un
  aviso.
- **RPM:** el contenido en LZMA antiguo (no XZ) y las cabeceras CPIO `07070X`
  de los paquetes con ficheros de más de 4 GiB.
- **DEB:** los paquetes del formato antiguo 0.939 y `data.tar.lzma`.
- **LZH:** `-lhx-` y las demás extensiones de UNLHA32.
- **CPIO comprimido:** un `initrd.img` es un CPIO dentro de gzip, a menudo detrás de un CPIO de microcódigo sin comprimir, y no se abre como CPIO; un `.cpio.gz`, `.cpio.xz`, `.cpio.bz2` o `.cpio.zst` se abre como un único fichero comprimido. Hay que descomprimirlo antes.
- **Los nombres LZH** en una página de códigos, normalmente Shift-JIS en los archivos japoneses, se muestran con cada byte que no es ASCII como `%xx`.
- **WIM** y **ARJ** no están soportados. Todavía no hay un lector de WIM en
  Rust puro que se mantenga: los crates publicados son primeras versiones, y WIM
  necesita además descompresores XPRESS y LZMS.

## Seguridad

Los lectores de CPIO, AR, DEB, RPM y CAB son de Arca, en Rust seguro. LZX viene
del crate [`lzxd`](https://crates.io/crates/lzxd) y LZH de
[`delharc`](https://crates.io/crates/delharc), los dos MIT o Apache-2.0, y
`delharc` se compila sin sus atajos sin comprobar. Un fichero dañado u hostil
da un error, nunca un cierre inesperado:

- cada tamaño se contrasta con el fichero o con el flujo: un miembro que declara
  más datos de los que hay falla en cuanto se acaban, y nunca se reserva memoria
  a partir de un tamaño declarado;
- los nombres tienen un máximo de 4096 bytes, las entradas los diez millones de
  siempre, las tablas de nombres de `ar` 16 MiB, las cabeceras RPM los límites
  del propio rpm y los bloques CAB 32 KiB;
- los nombres pasan por las mismas comprobaciones que en los demás formatos, así
  que `../` y las letras de unidad se rechazan al extraer.

Cada fichero se escribe con un nombre temporal y se renombra cuando está
completo. Las vistas previas tienen un máximo de 64 MiB.

## Contrastarlo con otras herramientas

Los cabinets de prueba de `arca-extra/tests/fixtures` se hicieron con `makecab`
de Windows (MSZIP y LZX) y `bsdtar` también los lista. Los lectores de CPIO y
AR se comprobaron a mano con archivos escritos por `bsdtar --format newc`,
`odc`, `bin`, `ar` y `argnu`.
