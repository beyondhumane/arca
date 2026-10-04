---
description: Leer imágenes de disco ISO 9660 (Rock Ridge, Joliet, ficheros multiextensión) sin escribir nunca una.
group: Referencia
order: 21
keywords: iso 9660 imagen de disco rock ridge joliet udf solo lectura cd dvd iso disc image read-only
---

# Imágenes de disco ISO

Arca lee imágenes ISO 9660 (`.iso`): imágenes de CD/DVD, medios de instalación
de Linux, paquetes de software. Viene incluido, no necesita ninguna feature de
Cargo y es de **solo lectura**: Arca nunca crea ni modifica una ISO.

```sh
arca list ubuntu.iso
arca test ubuntu.iso
arca extract ubuntu.iso -o ubuntu/
```

En la ventana, abre la imagen como cualquier archivo. El título indica
`ISO: read-only`; la vista previa, la extracción y la comprobación funcionan, y
añadir, borrar, renombrar, mover, crear carpeta y contraseña se rechazan antes
de tocar la imagen.

## Qué se lee

- **Nombres:** primero Rock Ridge (`NM`), luego Joliet (UTF-16) y luego ISO 9660
  simple, quitando la versión `;1` y el punto final.
- **Directorios reubicados por Rock Ridge** (`CL`/`RE`, el truco de `rr_moved`
  para árboles profundos) aparecen en su sitio real.
- **Ficheros de más de 4 GiB**, guardados en varias extensiones, salen como un
  único fichero.
- **Fechas** de los registros de directorio.

ISO 9660 no guarda sumas de comprobación. `arca test` demuestra que se puede
leer cada byte de cada fichero de la imagen, no que esos bytes sean los
originales.

## Qué se deja fuera, y se avisa

- **Enlaces simbólicos, dispositivos, FIFOs y sockets** de Rock Ridge no se
  listan ni se extraen. La CLI escribe `note: skipped N symbolic links or special
  files: ...` en stderr; la ventana muestra la misma línea.
- **UDF.** Las imágenes que además llevan UDF (DVD/Blu-ray, medios de instalación
  de Windows) se leen por su lado ISO 9660, con un aviso de que los ficheros que
  solo están en UDF no aparecen. En los medios de Windows `sources/install.wim`
  suele ser uno de ellos. Las imágenes solo UDF se rechazan.
- No soportado: imágenes de arranque El Torito como ficheros, ficheros
  asociados, ficheros intercalados, bloques lógicos distintos de 2048 bytes,
  montar la imagen.

## Seguridad

El parser es Rust seguro y sin dependencias. Una imagen dañada da un error,
nunca un fallo. En cada imagen se comprueba:

- descriptores de volumen, truncamiento y extensiones más allá del final del
  fichero;
- registros de directorio mal formados o que cruzan un límite de sector;
- bucles de directorios, anidamiento de más de 64 niveles, más de 256 MiB de
  datos de directorio y cadenas de continuación Rock Ridge de más de 32 áreas o
  64 KiB;
- nombres que escapan del destino o que no son seguros en Windows (`..`, `/`,
  `\`, letras de unidad, `:<>"|?*`, caracteres de control, `CON`, `NUL`, `COM1`,
  puntos o espacios finales);
- rutas duplicadas, también las que solo cambian en mayúsculas, y colisiones
  entre fichero y directorio.

La extracción copia cada fichero en bloques de 256 KiB a un fichero temporal
junto a su destino y lo renombra a su sitio al terminar. Cada carpeta se abre
por handle sin seguir enlaces, así que un enlace que ya esté dentro del árbol de
destino, o que aparezca durante la extracción, se rechaza en vez de seguirse. Se respetan omitir,
renombrar, sobrescribir y cancelar en los conflictos. Un fallo o una
cancelación puede dejar los ficheros que ya estaban terminados. La vista previa
está limitada a 64 MiB.

## Comprobarlo contra otras herramientas

`bash interop.sh` crea imágenes con `genisoimage` y `xorriso` (simple, Joliet,
Rock Ridge, directorios reubicados) y compara byte a byte lo que extrae Arca con
`bsdtar` y `7z`. Con `ARCA_TEST_ISO_BIG=1` también crea una imagen dispersa de
4,7 GB y comprueba un fichero multiextensión; necesita unos 5 GB libres.
