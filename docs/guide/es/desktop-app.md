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

El diálogo de creación tiene un campo de contraseña. Abrir un archivo cifrado pide la contraseña antes de extraer, y la barra de herramientas ofrece **Quitar contraseña** o **Establecer contraseña…** según el archivo.

## Lo que falta a propósito

<kbd>Ctrl+V</kbd> no está, y tampoco lo está <kbd>Ctrl+C</kbd> copiando archivos al portapapeles. Pegar significaría añadir a un archivo que ya existe, algo que el escritor todavía no puede hacer; copiar archivos hacia fuera significa entregar al shell un objeto del que pueda extraer bytes bajo demanda. Ambas cosas necesitan trabajo real más allá de un atajo de teclado, así que se dejan fuera hasta que funcionen.

## Accesibilidad

La ventana se declara a través de AccessKit, para que Narrator y NVDA puedan leerla. Cada acción tiene una vía por teclado; consulta [Atajos de teclado](keyboard-shortcuts.md).
