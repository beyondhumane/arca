---
description: Navega por archivos comprimidos como si fueran carpetas, en una ventana nativa rápida y pensada para el teclado.
group: Escritorio
order: 8
keywords: interfaz gráfica ventana arca-gui escritorio aplicación accesibilidad gui window arca-gui desktop app gpui accessibility nvda narrator
---

# El espacio de trabajo de escritorio

`arca-gui` permite explorar el disco y los archivos comprimidos en una ventana nativa GPUI. La barra de título conserva la marca y los controles de ventana de Arca; una sola banda de navegación reúne historial, ruta, vistas, filtro, extracción, comprobación y el menú **Más**.

## Explorar el disco

Al arrancar sin un archivo, la ventana muestra la última carpeta visitada (o la carpeta personal) como un explorador de archivos normal. Las carpetas se abren en las mismas vistas Columnas o Detalles que los archivos comprimidos; cada carpeta se lee al entrar en ella, no por adelantado. La barra lateral enumera **Lugares** (la carpeta personal y las carpetas de usuario habituales), las carpetas **Fijadas** y los **Dispositivos** (los volúmenes montados, o las unidades en Windows). Haz clic en uno para saltar allí; haz clic derecho en una fila de carpeta y elige **Fijar en la barra lateral** para tenerla a mano, y clic derecho sobre una entrada fijada para quitarla. **Más > Vista > Mostrar archivos ocultos** alterna los archivos ocultos.

Hacer doble clic en un archivo de un formato compatible (`.zip`, `.7z`, `.tar`, `.iso` y los contenedores ZIP) lo abre en el mismo sitio: la ruta, el árbol y los paneles pasan a su contenido y las acciones habituales de archivo quedan disponibles. **Arriba** o **Retroceso** en la raíz del archivo, o el botón **Cerrar archivo**, vuelven a la carpeta de origen con el cursor sobre el archivo. Los demás archivos se abren con la aplicación del sistema; la vista previa muestra los archivos locales igual que las entradas de un archivo comprimido.

En el disco, **Comprimir** usa las filas marcadas como entrada, **Copiar ruta** pone las rutas absolutas en el portapapeles y las acciones exclusivas de archivo (extraer, comprobar, renombrar, eliminar, pegar, nueva carpeta) permanecen desactivadas. La última carpeta visitada, las carpetas fijadas y la preferencia de archivos ocultos se recuerdan entre sesiones.

## Navegación y distribución

**Abrir** y **Crear** están en la barra lateral, junto a los archivos recientes y, cuando hay un archivo abierto, su árbol de carpetas. El botón lateral alterna entre panel ampliado, columna de iconos y oculto. Arrastra el divisor para ajustar su ancho. **Más > Vista** también permite ocultarlo.

La vista predeterminada **Columnas** mantiene un directorio en cada panel vertical. Haz clic en una carpeta para abrir su panel hijo; los antecesores permanecen a la izquierda. Elegir otra carpeta sustituye solo los descendientes. Cada panel conserva su propio cursor, selección, desplazamiento, filtro y orden. Haz clic o lleva el foco al panel antes de operar sobre él. Arrastra los divisores para ajustar el ancho y desplázate horizontalmente para volver a los antecesores. La navegación deja a la vista el panel activo.

La ruta permite ir a la raíz del archivo o a una carpeta superior. Un menú contiene los segmentos intermedios cuando no caben. El árbol y la ruta siguen al panel activo. Los botones de atrás/adelante y **Alt+Izquierda/Derecha** comparten el historial.

**Detalles** conserva la tabla de metadatos ordenable. Haz clic derecho en su cabecera para elegir las columnas. **Más > Vista > Vista plana** pasa a Detalles y muestra las entradas sin jerarquía. Cambiar de vista mantiene la ubicación y la selección que siga siendo válida.

Se recuerdan las preferencias y los anchos. Al estrechar la ventana, la barra lateral se reduce temporalmente a iconos y la vista previa se suspende antes de apretar el explorador. Al ampliarla se recuperan los paneles preferidos.

## Selección y operaciones

- Haz clic en un archivo para seleccionarlo; Ctrl+clic alterna su selección y Mayús+clic selecciona un intervalo. Usa estos modificadores para seleccionar carpetas sin abrirlas en Columnas.
- **Espacio** alterna la fila del cursor; **Ctrl+A** selecciona las filas visibles. Las operaciones de selección solo afectan al panel activo. La fila de carpeta superior de Detalles nunca es un destino de esas operaciones.
- Haz clic derecho en una fila para extraer, previsualizar, renombrar, eliminar y ver otras acciones aplicables. El espacio vacío del panel ofrece acciones sobre ese directorio.
- Arrastra entradas a una carpeta o panel de directorio para moverlas dentro de un ZIP modificable. Suelta archivos externos sobre un directorio para añadirlos. El destino se resalta durante el arrastre.
- El diálogo de nueva carpeta y los menús de pegado indican el destino. Las modificaciones ZIP se ejecutan de una en una; los RAR/CBR existentes siguen siendo de solo lectura.
- Copiar, cortar, pegar archivos y arrastrarlos fuera conservan la integración existente con Windows. **Ctrl+Mayús+C** copia los nombres como texto.
- **Más** reúne selección, cambios de contraseña, verificación, deshacer, ajustes y ayuda de atajos.

Hacer doble clic en un archivo, o pulsar **Intro** sobre él, lo extrae explícitamente a una ubicación temporal y lo abre con la aplicación del sistema. La vista previa no hace esto.

## Vista previa integrada

El panel derecho sigue al cursor del archivo activo. **F3** activa la vista previa; el botón de cierre la oculta y devuelve el foco. El pie permite mostrarla de nuevo. Arrastra su divisor para cambiar el ancho recordado.

El panel muestra nombre, tamaño, fecha y tipo, e indica expresamente los metadatos no disponibles. El texto usa una lista monoespaciada con números de línea; las vistas hexadecimal y de imágenes compatibles comparten el panel. Las imágenes se ajustan a él.

Los estados de carga, vacío, contraseña necesaria, contraseña incorrecta, formato no compatible, tamaño excesivo y error aparecen en el propio panel. La lectura, preparación de texto y decodificación de imágenes se ejecutan en segundo plano con límites, independientemente de las operaciones de modificación. Cambiar la navegación o selección invalida los resultados anteriores. Las contraseñas y el contenido no se guardan en los ajustes.

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
