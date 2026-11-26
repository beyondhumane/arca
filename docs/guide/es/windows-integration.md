---
description: El menú contextual del Explorador de archivos y el instalador de Windows.
group: Escritorio
order: 10
keywords: windows explorador menú contextual clic derecho instalador inno setup msix shell windows explorer context menu right click installer inno setup msix shell
---

# Integración con Windows

[English](../windows-integration.md) | **Español**

En Windows, Arca se integra con el Explorador de archivos para que puedas abrir y extraer archivos comprimidos sin tocar una terminal.

## Menú contextual del Explorador

`windows/` contiene la extensión del Explorador, en sus dos variantes:

| Menú | Construido sobre |
| --- | --- |
| Menú moderno de Windows 11 | `IExplorerCommand` más un paquete MSIX disperso |
| Menú clásico (Mostrar más opciones) | `IContextMenu` más claves de registro |

Ambas se destinan a Windows 11. Ofrecen **Abrir con Arca**, **Extraer aquí** y
**Extraer en "nombre\"** para todos los formatos que Arca lee, incluidos los
contenedores ZIP como `.jar` o `.epub` y los volúmenes RAR como `.r00`, y
nombran la carpeta exactamente como lo hará la ventana. **Añadir al archivo…**
funciona con cualquier selección.

## Progreso, no silencio

Hacer clic derecho en un archivo comprimido abre la ventana de Arca con una barra de progreso en lugar de ejecutar la línea de comandos sin consola. Una extracción que falla, o que encuentra un archivo que ya existe, lo indica en lugar de no hacer nada.

## Instalador

El instalador se genera con Inno Setup desde `windows/arca.iss`. Descarga `arca-setup-<version>-x86_64.exe` desde la [última versión](https://github.com/beyondhumane/arca/releases/latest), o lee [windows/README.md](https://github.com/beyondhumane/arca/tree/main/windows) para compilarlo tú mismo.

### Asociaciones de archivos

Con la opción de asociaciones activada, el instalador añade Arca a **Abrir con**
y a **Configuración > Aplicaciones predeterminadas** para estas extensiones:

| Extensiones | Formatos |
| --- | --- |
| `.zip`, `.7z`, `.rar`, `.tar` | ZIP, 7z, RAR, TAR |
| `.gz`, `.tgz` | GZ y TAR.GZ |
| `.xz`, `.txz` | XZ y TAR.XZ |
| `.bz2`, `.tbz2`, `.tbz` | BZ2 y TAR.BZ2 |
| `.zst`, `.tzst` | ZST y TAR.ZST |
| `.cpio`, `.cab`, `.lzh`, `.lha` | CPIO, CAB y LZH, de solo lectura |
| `.iso` | Imágenes de disco ISO, solo si se piden |

Windows se fija solo en la última extensión, así que para él `copia.tar.xz` es
un fichero `.xz`; Arca lee el nombre completo y lo abre como TAR.XZ.

Arca nunca se hace predeterminada por su cuenta: se registra junto a lo que ya
abre esos ficheros, y eres tú quien la elige en **Abrir con** o en
Configuración. El instalador nuevo, `arca-setup`, hace una excepción: toma
`.zip` para tu cuenta de usuario, para que aparezca **Nuevo > Archivo ZIP de
Arca** en el Explorador, y se lo devuelve a quien lo tuviera antes al
desinstalar Arca.

Las imágenes de disco vienen desactivadas, porque el Explorador ya las monta.
Marca la casilla de las imágenes de disco (.iso) en el instalador de Inno Setup,
abre con `--iso` el instalador nuevo que genera `windows/build-setup.ps1` o,
en la instalación de desarrollo, ejecuta `windows\install.ps1 -Iso`. Si vuelves
a instalar sin ellas, se retira el registro de las imágenes de disco que hizo
una instalación anterior.

Los paquetes y documentos basados en ZIP (`.jar`, `.apk`, `.epub`, `.cbz` y el
resto de [contenedores ZIP](zip-containers.md)) y los cómics `.cbr` nunca se
asocian: son de Java, de las herramientas de Android y de los lectores de
libros y cómics. Arca los abre cuando se lo pides, desde el menú contextual o
desde la ventana.

La lista sale de `arca_core::Format`. `arca-setup` la calcula al ejecutarse, y
sus pruebas fallan si `windows/arca.iss`, `windows/install.ps1` o la extensión
del Explorador en `windows/arca-shell` no coinciden con ella.

## Por qué vive fuera del workspace

`windows/arca-shell` necesita COM, y por tanto `unsafe`, y solo compila en Windows. Está excluido del workspace de Cargo para que `cargo build` siga funcionando en Linux y macOS.
