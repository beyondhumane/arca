# Instalador propio (`arca-setup`)

Estado: **etapas 1 y 2 escritas, la 2 a medio probar.** El instalador nuevo existe y se puede construir con `windows\build-setup.ps1`, pero las releases siguen publicando el de Inno Setup (`windows/arca.iss`) hasta que la etapa 4 lo sustituya.

## Qué hay hoy

`arca-setup` es un crate del workspace con una ventana GPUI de 720 × 420 que sigue la propuesta «Monolito fluido»: bienvenida, personalizar, instalando, listo y error, en español e inglés según el idioma del sistema, con Sora e Inter incrustadas y todo el arte en vectorial. Detrás hay un motor que porta lo que hacía `arca.iss`.

| Pantalla | Qué hace |
| --- | --- |
| Bienvenida | Logotipo horizontal, «Instalar ARCA» y «Personalizar» (o la confirmación, al desinstalar) |
| Personalizar | Tres interruptores (menú contextual, asociaciones, PATH) y la carpeta de instalación |
| Instalando | Barra, porcentaje y cuatro pasos con su estado |
| Listo | «Comenzar a usar ARCA» y enlace a las novedades |
| Error | El motivo y «Reintentar» |

El motor, en `arca-setup/src/engine`, hace en Windows y por usuario, sin administrador:

- Extrae los ficheros de un ZIP incrustado en el ejecutable con `arca-zip`. Si uno está en uso (Arca abierta, la DLL del Explorador cargada) lo aparta como `nombre.old-N` y escribe el nuevo; el barrido siguiente borra los apartados.
- Escribe el registro de HKCU: menú clásico, asociaciones, capacidades, PATH y la entrada de Aplicaciones instaladas. Con la clave de Inno, así que adopta una instalación previa en lugar de duplicarla.
- Registra el menú moderno con `Add-AppxPackage`, crea el acceso directo del menú Inicio y avisa al sistema del cambio de PATH y de asociaciones (`arca-notify`).
- Se copia como `unins000.exe`. `--uninstall` se relanza desde `%TEMP%` para poder borrar su propia carpeta y borra su copia al terminar.
- Al actualizar y en el modo silencioso conserva lo que el usuario tenía activado, en vez de volver a marcarlo todo.

### Qué está probado y qué no

Probado sin tocar el registro (`--files-only`, con una carpeta temporal): instalar, actualizar con un fichero bloqueado, el barrido, desinstalar sin dejar copia en `%TEMP%` y un fallo que devuelve el código 1 y queda en `%TEMP%\arca-setup.log`. Las funciones puras (PATH, base64 de PowerShell, manifiesto, opciones de la línea de órdenes, lectura del ZIP incrustado con ficheros truncados) tienen pruebas unitarias.

**Sin probar de punta a punta**: las escrituras reales en HKCU, el registro del paquete MSIX, el reinicio del Explorador y la actualización silenciosa desde la propia Arca. El ciclo pendiente es instalar y desinstalar comparando el registro y el PATH antes y después.

### Opciones de la línea de órdenes

Las de Inno que usa el actualizador (`/VERYSILENT`, `/SILENT`, `/update=1`, `/NOCANCEL`, `/NORESTART`, `/NORESTARTAPPLICATIONS`) más `/DIR=`, `--uninstall`, `--lang=es|en` y `--files-only`, que solo copia ficheros y sirve para probar. Con `--preview`, `--screen=welcome|options|installing|done|failed` o `--percent=N` la ventana simula el progreso sin instalar nada:

```sh
cargo run -p arca-setup -- --screen=installing --percent=68 --lang=es
```

### Cómo construirlo

```powershell
.\windows\build-setup.ps1        # release, con LTO
.\windows\build-setup.ps1 -Fast  # sin LTO, para probar
```

Deja `dist\arca-setup-<versión>-x86_64.exe`. Un `cargo build` a secas construye el instalador vacío a propósito: el ZIP con los binarios llega por la variable `ARCA_PAYLOAD`.

## Por qué no seguir con Inno

El asistente de Inno Setup no puede dibujar estas pantallas: fondo vectorial, pasos con estado, tipografía propia y escalado limpio a cualquier DPI. La cabecera de `windows/arca.iss` avisaba de que escribirlo a mano en Rust era rehacer, peor, lo que Inno ya hace bien. Sigue siendo verdad para la parte que no se ve, y por eso la etapa 2 es la delicada.

## Contrato que el instalador nuevo tiene que respetar

Arca se actualiza sola y da por hecho cómo es el instalador. Cambiarlo sin tocar estas cinco cosas rompe la actualización de quien ya lo tiene instalado.

1. **Nombre del fichero**: `arca-setup-<versión>-x86_64.exe`, subido a la release de `github.com/beyondhumane/arca`. El aviso de versión nueva busca ese patrón (`arca-gui/src/controller/actions.rs`) y comprueba la suma en `SHA256SUMS.txt`.
2. **Argumentos**: `/VERYSILENT /NOCANCEL /NORESTART /NORESTARTAPPLICATIONS /update=1` (`install_update` en `arca-gui/src/archive_ops/mod.rs`). En modo silencioso no se abre ninguna ventana.
3. **Actualización sin parpadeo**: con `/update=1` no se reinicia el Explorador. La DLL de la extensión se reemplaza en el siguiente arranque y los ejecutables se copian igualmente.
4. **Volver a abrir Arca**: con `/update=1` el propio instalador lanza `arca-gui.exe` al terminar; Arca se cierra sola para dejarse reemplazar.
5. **`unins000.exe` junto a `arca-gui.exe`**: `installed_by_setup()` lo usa para saber si la copia puede actualizarse sola. O el desinstalador nuevo se llama igual, o esa comprobación cambia en el mismo commit.

Además, hay que **adoptar una instalación de Inno ya existente**: mismo `AppId` (`{7C4E0E4A-6C0D-4C21-9E0B-2B5D0F1A9C77}`), misma carpeta `%LOCALAPPDATA%\Programs\Arca` y la clave de desinstalación que Inno dejó, para que actualizar no deje dos entradas en Aplicaciones instaladas.

## Qué hace `arca.iss` y hay que portar

| Función | Dónde está en `arca.iss` |
| --- | --- |
| Copiar `arca.exe`, `arca-gui.exe`, `arca_shell.dll`, manifiesto, licencia y `Assets` | `[Files]` |
| Instalación por usuario, sin administrador, sin UAC | `PrivilegesRequired=lowest` |
| Menú contextual clásico (CLSID en HKCU y dos manejadores) | `[Registry]`, tarea `shellmenu` |
| Menú moderno de Windows 11 (paquete MSIX disperso) | `RegisterModernMenu`, `StampManifestVersion` |
| Asociaciones `.zip`, `.tar`, `.gz`, `.tgz` con ProgID, `OpenWithProgids` y `Capabilities` | `[Registry]`, tarea `fileassoc` |
| Añadir y quitar `arca` del PATH del usuario | `AddToPath`, `RemoveFromPath` |
| Reiniciar el Explorador para soltar `arca_shell.dll` | `RestartExplorer`, `QuietUpgrade` |
| Entrada en Aplicaciones instaladas y desinstalador | `Uninstall*` |
| Borrar los `UserChoice` que apunten a Arca al desinstalar | `ForgetDefaults` |
| Acceso directo en el menú Inicio | `[Icons]` |

## Etapas

- [x] **1. Diseño y flujo.** Ventana, marca, fondos, tipografía, idiomas, teclado. Sin efectos en el sistema.
- [x] **2. Motor.** `Choices` entra, eventos de progreso salen; la carga útil viaja como un ZIP dentro del ejecutable y se extrae con `arca-zip`. Escrito y probado con `--files-only`.
- [ ] **3. Probarlo de verdad.** Ciclo completo de instalar y desinstalar comparando HKCU y PATH antes y después, la actualización silenciosa que lanza Arca sobre una instalación de Inno, y el menú moderno con el modo de desarrollador activo.
- [ ] **4. Release.** Con la 3 hecha, sustituir el paso de Inno en `.github/workflows/release.yml`, comprobar `--version` del instalador silencioso y probar una actualización real de una versión anterior.
- [ ] **5. Retirar `arca.iss`** cuando la etapa 4 haya salido en una release sin incidencias.

## Decisiones ya tomadas

- **GPUI, no otra pila.** Es la que ya compila y empaqueta el proyecto; comparte dependencias con `arca-gui` y el caché de CI.
- **SVG con `resvg` de GPUI.** El símbolo, el logotipo y los fondos son vectoriales. Cada SVG lleva un tamaño intrínseco cercano a 1,5 veces el de pantalla porque GPUI lo rasteriza al doble: mucho más grande y el muestreo produce dientes de sierra.
- **Fondos sin filtros de desenfoque.** Cada `feGaussianBlur` obligaba a rasterizar un lienzo entero y la pantalla tardaba segundos en pintar el fondo.
- **Sora e Inter incrustadas.** Sora SemiBold (títulos, botones, énfasis) e Inter Regular (texto), ambas con licencia OFL, que viaja en `arca-setup/assets/fonts`. Si no se cargan, el instalador cae a la fuente del sistema.
- **El eslogan va en contornos**, extraídos de los glifos de Sora, para que el logotipo se vea igual sin la fuente instalada.
