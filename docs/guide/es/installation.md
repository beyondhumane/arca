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

El `.deb` y el `.rpm` instalan los dos binarios en `/usr/bin` y añaden Arca al menú de aplicaciones:

```sh
sudo apt install ./arca_*_amd64.deb      # Debian, Ubuntu
sudo dnf install ./arca-*.x86_64.rpm     # Fedora
sudo zypper install ./arca-*.x86_64.rpm  # openSUSE
```

La AppImage no necesita instalación. Dale permiso de ejecución y ábrela; con `--cli` se ejecuta la línea de comandos en lugar de la ventana:

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
| codecs-native (por defecto) | `cargo build --release` | Incluye libzstd (C) para el rendimiento nativo de Zstandard. |
| Rust puro | `cargo build --release --no-default-features` | Sin dependencia de C; compila para cualquier destino que soporte Rust. |

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
