---
description: El menú contextual del Explorador de archivos y el instalador de Windows.
group: Escritorio
order: 10
keywords: windows explorador menú contextual clic derecho instalador inno setup msix shell windows explorer context menu right click installer inno setup msix shell
---

# Integración con Windows

En Windows, Arca se integra con el Explorador de archivos para que puedas abrir y extraer archivos comprimidos sin tocar una terminal.

## Menú contextual del Explorador

`windows/` contiene la extensión del Explorador, en sus dos variantes:

| Menú | Construido sobre |
| --- | --- |
| Menú moderno de Windows 11 | `IExplorerCommand` más un paquete MSIX disperso |
| Menú clásico (Mostrar más opciones) | `IContextMenu` más claves de registro |

Ambas se destinan a Windows 11. La integración `.7z` necesita verificación en
Windows, por separado de las comprobaciones de terminal en Linux.

## Progreso, no silencio

Hacer clic derecho en un archivo comprimido abre la ventana de Arca con una barra de progreso en lugar de ejecutar la línea de comandos sin consola. Una extracción que falla, o que encuentra un archivo que ya existe, lo indica en lugar de no hacer nada.

## Instalador

El instalador se genera con Inno Setup desde `windows/arca.iss`. Descarga `arca-setup-<version>-x86_64.exe` desde la [última versión](https://github.com/beyondhumane/arca/releases/latest), o lee [windows/README.md](https://github.com/beyondhumane/arca/tree/main/windows) para compilarlo tú mismo.

El instalador registra `.7z` junto a ZIP/TAR/gzip para Abrir con y seleccionar
aplicaciones predeterminadas. El Explorador reconoce `.7z` para Abrir/Extraer y
quita la extensión al nombrar el destino. Añadir a archivo crea uno nuevo con
la selección; no modifica el 7z seleccionado. El diálogo de creación ofrece 7z.

## Por qué vive fuera del workspace

`windows/arca-shell` necesita COM, y por tanto `unsafe`, y solo compila en Windows. Está excluido del workspace de Cargo para que `cargo build` siga funcionando en Linux y macOS.
