---
description: Descarga un binario precompilado para Windows, macOS o Linux, o compila Arca desde el código fuente con Cargo.
group: Primeros pasos
order: 2
keywords: instalar descargar compilar cargo rust versión binario windows macos linux setup exe install download build cargo rust release binary windows macos linux setup exe
---

# Instalación

Cada versión publica binarios precompilados para Windows, macOS y Linux. También puedes compilar Arca desde el código fuente con un toolchain de Rust estándar.

## Descargar una versión

Consigue el archivo para tu plataforma desde la [última versión](https://github.com/beyondhumane/arca/releases/latest):

| Plataforma | Archivo | Notas |
| --- | --- | --- |
| Windows | `arca-setup-<version>-x86_64.exe` | Instalador, x86\_64 |
| Windows | `arca-v<version>-windows-x86_64.zip` | Portable, sin instalador |
| macOS | `arca-v<version>-macos-arm64.tar.gz` | Apple silicon |
| macOS Intel | `arca-v<version>-macos-x86_64.tar.gz` | Intel |
| Linux | `arca-v<version>-linux-x86_64.tar.gz` | x86\_64 |
| Linux ARM | `arca-v<version>-linux-arm64.tar.gz` | arm64 |
| Linux | `arca-v<version>-linux-x86_64.AppImage` | Ventana de escritorio, cualquier distribución; también `linux-arm64` |
| Debian, Ubuntu | `arca_<version>-1_amd64.deb` | Paquete; también `arm64` |
| Fedora, openSUSE | `arca-<version>-1.x86_64.rpm` | Paquete; también `aarch64` |

Todos los archivos de todas las versiones, con su hash SHA-256, están en la [página de versiones](https://github.com/beyondhumane/arca/releases).

### Windows

Ejecuta el instalador. Está generado con Inno Setup desde `windows/arca.iss` y configura Arca junto con el menú contextual del Explorador; ver [Integración con Windows](windows-integration.md).

### Paquetes de Linux

El `.deb` y el `.rpm` instalan los dos binarios en `/usr/bin` e integran Arca en el escritorio:

```sh
sudo apt install ./arca_*_amd64.deb      # Debian, Ubuntu
sudo dnf install ./arca-*.x86_64.rpm     # Fedora
sudo zypper install ./arca-*.x86_64.rpm  # openSUSE
```

| Ruta | Para qué sirve |
| --- | --- |
| `/usr/share/applications/arca.desktop` | Arca en el menú de aplicaciones y en **Abrir con** para ZIP, 7z, RAR, TAR, TAR.GZ, XZ, BZ2 y ZST (solos o envolviendo un TAR) e imágenes ISO |
| `/usr/share/icons/hicolor/*/apps/dev.beyondhumane.Arca.*` | El icono de 16 a 256 px, más el SVG |
| `/usr/share/kio/servicemenus/arca-*.desktop` | Un submenú **Arca** en Dolphin: **Extraer aquí** y **Extraer en una carpeta nueva** en los archivos comprimidos, **Añadir al archivo…** y **Comprimir en ZIP** en cualquier cosa |

Instalar nunca cambia qué programa abre cada tipo por defecto: elige Arca en **Abrir con**, o hazla predeterminada en la configuración de tu escritorio. Los paquetes y documentos basados en ZIP, como `.jar`, `.apk` o `.epub`, no se reclaman; Arca los sigue abriendo desde la ventana. Todos los tipos de la lista tienen ya un nombre estándar en shared-mime-info, así que Arca no instala definiciones MIME propias. Tras instalar y tras desinstalar, el paquete actualiza las cachés de entradas de escritorio y de iconos si están `update-desktop-database` y `gtk-update-icon-cache`.

Otros gestores de archivos guardan sus entradas del menú contextual en tu propia configuración, así que un paquete no puede añadirlas:

- **Nautilus (Archivos de GNOME)**: guarda un script como este en `~/.local/share/nautilus/scripts/` y dale permiso de ejecución; aparece en **Scripts** en el menú del botón derecho. `--extract-to-folder`, `--add` y `--add-quick` dan las demás acciones.

  ```sh
  #!/bin/sh
  exec arca-gui --extract-here "$@"
  ```

- **Thunar**: en **Editar > Configurar acciones personalizadas…**, añade una acción con la orden `arca-gui --extract-here %F` (u otra de las opciones) y elige los tipos de archivo en **Condiciones de aparición**.

La AppImage no necesita instalación ni registra nada en el escritorio. Dale permiso de ejecución y ábrela; con `--cli` se ejecuta la línea de comandos en lugar de la ventana:

```sh
chmod +x arca-v*-linux-x86_64.AppImage
./arca-v*-linux-x86_64.AppImage
./arca-v*-linux-x86_64.AppImage --cli --help
```

### macOS y Linux

Descarga el tarball de tu plataforma, descomprímelo y pon el binario `arca` en algún lugar de tu `PATH`:

```sh
tar -xzf arca-v*-linux-x86_64.tar.gz
# luego mueve el binario arca a tu PATH, por ejemplo:
sudo mv arca /usr/local/bin/
```

En macOS la versión publicada son los dos binarios, no un paquete de aplicación, así que el Finder no puede ofrecer **Abrir con Arca** ni dar a los archivos comprimidos el icono de Arca. Para eso hace falta un `Arca.app` cuyo `Info.plist` declare los tipos de archivo (`CFBundleDocumentTypes`, más `UTImportedTypeDeclarations` para los formatos que macOS no conoce), y una compilación firmada y notarizada para que Gatekeeper la deje abrirse. El flujo de publicación todavía no genera nada de eso.

> [!TIP]
> **Gatekeeper de macOS**
>
> Si macOS se niega a ejecutar un binario descargado con el navegador, quita la marca de cuarentena con `xattr -d com.apple.quarantine ./arca`.

### Nix

En NixOS, o en cualquier Linux con Nix y los flakes activados, el repositorio es un flake. Compila los dos programas desde el código fuente e instala la entrada de escritorio:

```sh
nix run github:beyondhumane/arca              # abre la ventana
nix run github:beyondhumane/arca#arca -- --help
nix profile install github:beyondhumane/arca  # instala arca y arca-gui
```

`nix develop` abre una shell con el toolchain de Rust y las herramientas con las que comparan `interop.sh` y `bench.sh`.

## Compilar desde el código fuente

Necesitas Rust 1.95 o más reciente. La compilación por defecto también compila libzstd, así que debe haber un compilador de C disponible (cc, clang o MSVC).

```sh
git clone https://github.com/beyondhumane/arca
cd arca
cargo build --release      # el binario queda en target/release/arca
```

### Dos perfiles de compilación

| Perfil | Comando | Qué obtienes |
| --- | --- | --- |
| Por defecto | `cargo build --release` | Códecs Zstandard nativos y RAR de solo lectura. |
| Códecs Rust puro con RAR | `cargo build --release --no-default-features --features rar` | Sin códecs ZIP/7z nativos; conserva el lector RAR. |
| Sin opciones por defecto | `cargo build --release --no-default-features` | Sin códecs ZIP/7z nativos ni lector RAR. |

Usa `--no-default-features --features codecs-native` para excluir solo RAR.

### Verificar tu compilación

```sh
cargo test --workspace
bash interop.sh            # el criterio de aceptación de la fase
```

`interop.sh` hace ida y vuelta de los archivos con `zip`, `unzip`, `tar` y 7-Zip del sistema, así que ten esas herramientas instaladas primero. Ver [Interoperabilidad](interoperability.md) para saber qué comprueba.

### Sobre el perfil de release

Las compilaciones de release usan LTO completo, una sola unidad de generación de código, `panic = "abort"` y símbolos eliminados, ajustados para el requisito R1 (arranque) y R3/R4 (rendimiento).

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```
