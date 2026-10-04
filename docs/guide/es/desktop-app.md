---
description: Navega por archivos comprimidos como si fueran carpetas, en una ventana nativa rápida y pensada para el teclado.
group: Escritorio
order: 8
keywords: interfaz gráfica ventana arca-gui escritorio aplicación accesibilidad gui window arca-gui desktop app gpui accessibility nvda narrator
---

# El espacio de trabajo de escritorio

`arca-gui` permite explorar archivos comprimidos en una ventana nativa GPUI. La barra de título conserva la marca y los controles de ventana de Arca; una sola banda de navegación reúne historial, ruta, vistas, filtro, extracción, comprobación y el menú **Más**.

## Navegación y distribución

**Abrir** y **Crear** están en la barra lateral, junto a los archivos recientes y el árbol de carpetas del archivo actual. El botón lateral alterna entre panel ampliado, columna de iconos y oculto. Arrastra el divisor para ajustar su ancho. **Más > Vista** también permite ocultarlo.

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

## Límites por formato

Añadir, borrar, renombrar y cambiar contraseñas en archivos existentes sigue
siendo exclusivo de ZIP; 7z y RAR se pueden crear, pero no modificar después. Arrastrar ficheros virtuales fuera de un 7z está
desactivado para no descodificar repetidamente bloques sólidos: usa Copiar o
Extraer. El portapapeles de ficheros depende de la plataforma.

## Operaciones y teclado

Progreso, pausa, cancelación, finalización y errores se mantienen visibles sin bloquear la exploración cuando la operación permite navegar. Pausar y cancelar también actúan dentro de una sola entrada grande: una extracción cancelada borra el fichero que estaba escribiendo y conserva los que ya había terminado. No se inicia una segunda modificación mientras otra está en curso. Las operaciones puntuales del Explorador conservan su comportamiento al terminar.

El pie muestra directorio activo, cantidades visibles y seleccionadas, estado y ayuda **F1**. Tabulador llega a navegación, barra lateral, explorador, controles de vista previa y pie. Los campos conservan sus teclas de edición; los diálogos devuelven el foco al cerrarse. Los controles exponen etiquetas AccessKit e indicadores de foco y selección.

Consulta [Atajos de teclado](keyboard-shortcuts.md).
