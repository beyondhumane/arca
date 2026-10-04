---
description: Navega por archivos comprimidos como si fueran carpetas, en una ventana nativa rápida y pensada para el teclado.
group: Escritorio
order: 8
keywords: interfaz gráfica ventana arca-gui escritorio aplicación accesibilidad gui window arca-gui desktop app gpui accessibility nvda narrator
---

# La ventana de escritorio

La ventana `arca-gui` abre los archivos comprimidos como si fueran carpetas. Se renderiza por GPU con GPUI y está diseñada para poder manejarse por completo desde el teclado.

## Exploración

- Haz doble clic en una carpeta para entrar en ella.
- Haz doble clic en un archivo para sacar esa entrada a una carpeta temporal y pasársela a lo que tu sistema use para abrirlo.
- Toda la fila responde al ratón, no solo el nombre, y el cursor cambia para indicarlo.

## Navegación

Los botones de atrás y adelante del ratón se mueven por donde has estado, y también lo hacen <kbd>Alt+←</kbd> y <kbd>Alt+→</kbd>. Las tres flechas de la barra de herramientas hacen lo mismo, además de subir un nivel.

## Iconos nativos de archivo

Cada fila lleva el icono que tu escritorio muestra para ese tipo de archivo, así que un listado se parece al del gestor de archivos de al lado. En Windows es una única llamada al shell, pedida por nombre sin abrir nada, porque las entradas no existen en disco. En el resto de sistemas, la ventana dibuja sus propios iconos.

## Selección

- Clic para una entrada, Ctrl+clic para añadir o quitar una, Shift+clic para todo lo que hay en medio.
- Pulsa sobre la lista y arrastra para un rectángulo que selecciona todo lo que toca.
- Desde el teclado: <kbd>Space</kbd> marca una fila, <kbd>Shift+↓</kbd> marca una serie, y <kbd>Ctrl+A</kbd> marca todo, o lo desmarca si ya está todo marcado.

## Columnas y menús contextuales

Haz clic derecho en una fila para ver qué se puede hacer con ella. Haz clic derecho en la cabecera para activar o desactivar columnas (tamaño, comprimido, método, ahorrado, modificado y CRC32), y tu elección se recuerda entre ejecuciones. El nombre siempre se queda, así que la lista nunca muestra tamaños sin nombres.

## Contraseñas

El diálogo admite contraseñas ZIP y 7z y permite **Ocultar nombres** en 7z.
Las cabeceras 7z ocultas piden la contraseña antes de listar. Los datos se validan
en segundo plano antes de extraer, probar, previsualizar o abrir; reintentar retoma
esa acción y cancelar deja el destino intacto. **Quitar contraseña** y
**Establecer contraseña…** solo se aplican a ZIP, nunca a un 7z existente.

## 7z

Crea 7z con Store o LZMA2 y los niveles existentes. La creación y la extracción
son secuenciales. Las entradas seleccionadas de archivos sólidos comparten un
decodificador por bloque y pasada, sin reabrirlo por cada entrada. La vista previa,
abrir, Copiar y Extraer usan la misma ruta de lectura validada. Consulta
[Cifrado](encryption.md) y los [límites de recursos](architecture.md#7z-boundaries).

## XZ y TAR.XZ

El diálogo de crear ofrece TAR.XZ y XZ con LZMA2 y los niveles de siempre. XZ
admite un único fichero; con una carpeta o varios ficheros el diálogo pide
TAR.XZ. Un `.xz` suelto se abre como una lista con una entrada que se llama como
el archivo sin `.xz`. Probar, la vista previa, Copiar y Extraer leen el flujo
hasta el final y lo comprueban, y una creación detenida o fallida no deja un
archivo a medias.

## Lo que falta a propósito

Añadir, borrar, renombrar y cambiar contraseñas en archivos existentes sigue
siendo exclusivo de ZIP. Arrastrar ficheros virtuales fuera de un 7z está
desactivado para no descodificar repetidamente bloques sólidos: usa Copiar o
Extraer. El portapapeles de ficheros depende de la plataforma.

## Accesibilidad

La ventana se declara a través de AccessKit, para que Narrator y NVDA puedan leerla. Cada acción tiene una vía por teclado; consulta [Atajos de teclado](keyboard-shortcuts.md).
