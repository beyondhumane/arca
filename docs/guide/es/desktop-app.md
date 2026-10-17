---
description: Un explorador de archivos que abre los comprimidos en el mismo sitio, con paneles, vista previa, dispositivos y un clic derecho para comprimir o extraer.
group: Escritorio
order: 8
keywords: interfaz gráfica ventana arca-gui escritorio aplicación explorador paneles vista previa dispositivos accesibilidad gui window desktop app egui file explorer panes preview devices accessibility nvda narrator
---

# El espacio de trabajo de escritorio

`arca-gui` es un explorador de archivos y un archivador en la misma ventana. Arranca en la carpeta donde lo dejaste, muestra el disco como cualquier explorador y abre un archivo comprimido en el mismo sitio al hacer doble clic, con los mismos paneles, filtros, vista previa y menús contextuales. La banda superior reúne el historial, la ruta (haz clic en su parte vacía, en el lápiz o pulsa **Ctrl+L** para escribirla), el filtro, el menú **Vista**, la búsqueda (**Ctrl+K**), los ajustes y el menú **Más**.

## Explorar el disco

La barra lateral enumera **Lugares** (la carpeta personal y las carpetas de usuario habituales), las carpetas **Fijadas**, los **Dispositivos** y los archivos **Recientes**. Los dispositivos salen de la lista de volúmenes del sistema: unidades extraíbles, volúmenes montados bajo `/media`, `/run/media` o `/mnt`, y volúmenes con sistema de archivos que aún no están montados; en Windows, las letras de unidad. Un clic sobre un volumen sin montar lo monta primero. Con clic derecho sobre un volumen tienes **Montar** o **Desmontar**, **Formatear…** (tras un aviso; ext4, exFAT, FAT o NTFS, con etiqueta opcional) y **Poner etiqueta…**; en Linux pasan por UDisks, así que el escritorio puede pedirte la contraseña. **Más > Vista > Mostrar archivos ocultos** alterna los archivos ocultos.

Clic derecho sobre un archivo o carpeta para el menú completo: **Nueva carpeta con la selección**, **Abrir**, **Abrir con** (las aplicaciones que aceptan ese tipo), **Vista rápida**, **Cortar**, **Copiar**, **Pegar**, **Duplicar**, **Renombrar**, **Mover a…**, **Copiar a…**, **Comprimir**, **Copiar ruta**, **Copiar nombre**, **Seleccionar todo**, **Nueva carpeta**, **Fijar en la barra lateral**, **Propiedades**, **Mover a la papelera** (**Supr**) y **Eliminar permanentemente** (**Mayús+Supr**, con confirmación). Arrastra filas sobre una carpeta o sobre otro panel para moverlas.

Hacer doble clic en un archivo comprimido (`.zip`, `.7z`, `.tar`, `.tar.gz`, `.tar.xz`, `.xz`, `.rar`, `.iso` y los contenedores ZIP) lo abre en el mismo sitio. Dentro, el menú ofrece **Extraer selección**, **Extraer aquí**, **Extraer todo** y **Comprobar**, además de **Renombrar**, **Eliminar**, **Nueva carpeta** y **Pegar** en un ZIP; la contraseña se pide cuando el archivo la necesita. **Arriba** o **Retroceso** en la raíz, o **Cerrar archivo**, vuelven a la carpeta con el cursor sobre el archivo. Los demás archivos se abren con la aplicación del sistema.

## Paneles, detalles y barra lateral

La vista **Paneles** (la predeterminada) pone una carpeta en cada panel vertical. Cada panel tiene su propia cabecera con **Recargar**, **Subir**, **Ordenar**, **Filtrar** y **Cerrar**, y su propio cursor, selección, desplazamiento, filtro y orden. Tres ajustes en **Ajustes > Vistas** (también en el menú **Vista**) deciden cómo se comportan:

- **Abrir las carpetas en el panel de al lado** (`next_pane`, desactivado por defecto): activado, seleccionar una carpeta la abre en el panel de la derecha y los antecesores quedan a la izquierda, al estilo de columnas Miller; desactivado, cada panel navega por su cuenta y **Abrir en un panel nuevo** del menú contextual añade uno.
- **Asomarse a las carpetas al pasar el ratón** (`folder_peek`, desactivado por defecto): dejar el puntero sobre una fila de carpeta muestra una tarjeta con sus primeras entradas sin abrirla.
- **El foco sigue al puntero** (`pointer_focus`, activado por defecto): el panel bajo el puntero es el activo, de modo que la barra de ruta, el filtro, los atajos y las acciones actúan sobre él, sin clic y sin mover los paneles. Cualquier tecla pasa el mando al teclado hasta que el puntero vuelve a moverse; **Tab**, **Mayús+Tab**, **←** y **→** recorren los paneles. La fila de paneles solo se desplaza cuando la navegación abre un panel fuera de la vista, con su barra de desplazamiento o con **Mayús+rueda**.

**Detalles** es una tabla ordenable; clic derecho en su cabecera para elegir las columnas. **Lista plana** enumera todo lo que hay bajo la carpeta actual sin jerarquía. El **Filtro** (**Ctrl+F**) pertenece al panel en Paneles y a toda la lista en Detalles.

El botón de la barra lateral la muestra u oculta; cuando la ventana deja poco sitio para las carpetas de al lado, la barra se reduce sola a una columna de iconos y vuelve cuando hay espacio. Arrastra los divisores para ajustar la barra lateral, los paneles y la vista previa; todos los anchos se recuerdan.

## Selección y acciones

- Un clic selecciona, **Ctrl+clic** alterna, **Mayús+clic** selecciona un rango, **Espacio** alterna la fila del cursor y **Ctrl+A** selecciona las filas visibles del panel activo.
- **Comprimir** abre el cuadro de compresión: nombre del archivo con extensión automática, carpeta de destino, formato (ZIP, TAR, TAR.GZ, TAR.XZ, 7z, XZ o RAR), método, un deslizador de nivel 0–9, contraseña AES-256 opcional (con nombres ocultos en 7z) y la lista de lo que entra.
- **Extraer…** (**Ctrl+E**) pregunta dónde; **Extraer aquí** (**Alt+W**) descomprime junto al archivo.
- **Propiedades** muestra tamaño, fechas, tipo, permisos y el contenido de una carpeta.
- **Más** incluye herramientas de selección, cambio de contraseña, comprobación, ajustes, la lista de atajos y **Buscar actualizaciones**; una versión nueva abre un cuadro con sus notas y se instala desde la propia aplicación.

## Vista previa integrada

El panel derecho (**F3**) sigue al cursor: una cabecera con el icono del tipo, el nombre, el tamaño, la fecha y el tipo MIME, y botones para abrir el archivo con la aplicación del sistema, cambiar entre **Texto**, **Hex** e **Imagen**, ajustar las líneas y cerrar. El texto lleva números de línea y resaltado de sintaxis para shell, Rust, lenguajes tipo C, Python, JavaScript y TypeScript, JSON, TOML, YAML, INI, HTML y XML, CSS y Markdown; solo se maquetan las líneas visibles, así que los archivos grandes siguen fluidos. Hex muestra los desplazamientos en el margen y las imágenes se ajustan al panel. Los estados de carga, contraseña y error se muestran en el sitio; las contraseñas y el contenido de la vista previa nunca se escriben en los ajustes.

## Contraseñas

El diálogo admite contraseñas ZIP y 7z y permite **Ocultar nombres** en 7z.
Las cabeceras 7z ocultas piden la contraseña antes de listar. Los datos se validan
en segundo plano antes de extraer, probar, previsualizar o abrir; reintentar retoma
esa acción y cancelar deja el destino intacto. **Quitar contraseña** y
**Establecer contraseña…** solo se aplican a ZIP, nunca a un 7z existente.

## RAR

**Crear** ofrece RAR junto a ZIP y 7z y escribe un archivo RAR5 nuevo de un solo
volumen con el nivel elegido. Con RAR seleccionado el diálogo muestra el
compresor RAR fijo y una nota de que la salida nunca se cifra; el campo de
contraseña, **Ocultar nombres** y el selector de códec ZIP no aparecen, y una
contraseña escrita para ZIP o 7z se conserva para cuando vuelvas. Un nombre de
salida ya ocupado se rechaza antes de escribir nada, con un mensaje para elegir
otro nombre; no se ofrece Reemplazar porque la creación de RAR nunca sustituye
un fichero. El archivo nuevo se abre de solo lectura como cualquier
otro RAR, y Añadir, Borrar, Renombrar y contraseña siguen desactivados para él.
No se puede crear CBR. Consulta [RAR](rar.md#crear-archivos-rar5).

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

## Límites por formato

Añadir, borrar, renombrar y cambiar contraseñas en archivos existentes sigue
siendo exclusivo de ZIP; 7z y RAR se pueden crear, pero no modificar después. Arrastrar ficheros virtuales fuera de un 7z está
desactivado para no descodificar repetidamente bloques sólidos: usa Copiar o
Extraer. El portapapeles de ficheros depende de la plataforma.

## Operaciones y teclado

Cada operación (extraer, probar, comprimir, añadir, borrar, renombrar, cambiar contraseña, copiar) es una fila del panel **Operaciones**, que flota sobre la esquina inferior derecha del espacio de trabajo y se puede ocultar y reabrir desde el botón del pie que las cuenta. Cada fila muestra su progreso, tiempo transcurrido y restante, pausar/seguir y cancelar; las filas terminadas conservan su resultado hasta que se quitan. Pausar y cancelar también actúan dentro de una sola entrada grande: una extracción cancelada borra el fichero que estaba escribiendo y conserva los que ya había terminado.

Las operaciones corren a la vez cuando tocan ficheros distintos. Las reescrituras del mismo archivo se esperan entre sí en el orden en que se pidieron, y una reescritura espera también a cualquier extracción o prueba de ese archivo que ya esté en marcha. Mientras se reescribe el archivo abierto su listado es de solo lectura; extraer, probar o comprimir deja la ventana libre para seguir navegando y lanzar más trabajo. Las preguntas de sobrescritura indican a qué operación pertenecen. Las operaciones puntuales del Explorador muestran el mismo panel como ventana completa y conservan su comportamiento al terminar.

El pie muestra directorio activo, cantidades visibles y seleccionadas, estado y ayuda **F1**. Tabulador llega a navegación, barra lateral, explorador, controles de vista previa y pie. Los campos conservan sus teclas de edición; los diálogos devuelven el foco al cerrarse. Los controles exponen etiquetas AccessKit e indicadores de foco y selección.

Consulta [Atajos de teclado](keyboard-shortcuts.md).
