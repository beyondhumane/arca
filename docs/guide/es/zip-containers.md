---
description: Abre, lista, comprueba y extrae APK, JAR, EPUB, CBZ, wheels y otros archivos basados en ZIP sin renombrarlos, y por qué Arca los deja en solo lectura.
group: Referencia
order: 21
keywords: contenedor zip apk aar jar war ear epub cbz xpi whl wheel nupkg ipa solo lectura firma zip container read-only signature
---

# Contenedores ZIP

Muchos tipos de archivo son archivos ZIP normales con su propia extensión y
algunas reglas más. Arca los abre con su lector de ZIP, así que no hace falta
renombrarlos:

| Extensión | Qué es |
| --- | --- |
| `.apk`, `.aar` | App y biblioteca de Android |
| `.jar`, `.war`, `.ear` | Archivo Java, aplicación web y aplicación empresarial |
| `.epub` | Libro electrónico |
| `.cbz` | Cómic |
| `.xpi` | Complemento de Firefox |
| `.whl` | Wheel de Python |
| `.nupkg` | Paquete NuGet |
| `.ipa` | App de iOS |

```sh
arca list app.apk
arca test libro.epub                 # comprueba cada CRC, no escribe nada
arca extract comic.cbz -o paginas/
```

La ventana de escritorio también los abre y muestra el formato real en el
título, por ejemplo `app.apk - Arca (APK: read-only)`. Las extensiones no
distinguen mayúsculas, así que `LIBRO.EPUB` funciona igual.

## Solo lectura

Arca nunca crea ni modifica estos archivos. `arca create app.jar ...`,
`arca password` y añadir, borrar o renombrar entradas en la ventana se
rechazan con un error que nombra el formato.

Reescribirlos rompería lo que los hace algo más que un ZIP:

- Los APK, AAR, JAR, XPI e IPA suelen llevar una firma sobre sus entradas.
  Cualquier cambio la invalida y la app o el complemento ya no se instala.
- Un EPUB debe empezar con una entrada `mimetype` sin comprimir. Una
  reescritura que la reordene o la comprima da un libro que los lectores
  rechazan.
- Los wheels y los paquetes NuGet enumeran sus archivos y hashes en un
  manifiesto que tiene que coincidir con el contenido.

Para cambiar uno, extráelo, edita los archivos y vuelve a construirlo con la
herramienta de ese formato (`apksigner`, `jar`, `pip wheel`, `nuget pack`,
etc.).

Arca no comprueba el significado de estos formatos: no valida un EPUB, no
verifica firmas ni muestra páginas de cómic. Los lee como ZIP, con los mismos
límites y la misma protección contra entradas que intentan salir de la
carpeta de destino.
