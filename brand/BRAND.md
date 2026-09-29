# Marca de Arca

Azul noche: **#05060A → #1B2A4A**, degradado en diagonal de la esquina inferior izquierda
a la superior derecha.

## Geometria

Lienzo 64x64, cuadrado con radio 14.5.

| Pieza | Medidas |
|---|---|
| Tapa | x 10-54, y 14-24, radio 4 |
| Cuerpo | x 10-54, y 27-50, radio 4 |
| Maneta | x 21.5-42.5, y 34.8-42.2, radio 3.7 |

Tapa y cuerpo **comparten los dos bordes verticales** y el mismo radio. La maneta
va centrada en los dos ejes del cuerpo. Si tocas el glifo, respeta esos bordes.

La maneta va calada: deja ver el fondo a traves. Por eso la version plana funciona
con una sola tinta.

## Ficheros

| Fichero | Para que |
|---|---|
| `arca-isotipo.svg` | Icono del .exe y de los archivos asociados |
| `arca-isotipo-troquel.svg` | Alternativa: maneta troncoconica de caja de archivo |
| `arca-glifo-plano.svg` | Menu contextual y barras. Usa `currentColor` |
| `arca-logotipo.svg` | Isotipo mas la palabra, texto en #1B2A4A |
| `arca-logotipo-blanco.svg` | Igual, texto en blanco, para fondos oscuros |

## Pendiente

Convertir `arca-isotipo.svg` a `.ico` con 16, 24, 32, 48 y 256 dentro, para
sustituir los PNG de relleno que genera `windows/build.ps1`.

## Monolito fluido (propuesta)

Segunda marca, en evaluación: una «A» hecha de dos bandas paralelas que se cruzan en la cima, con un triángulo naranja en el hueco. Es la que usan el instalador (`arca-setup`), el icono de `arca.exe` y de `arca-gui.exe` (`arca-monolito.ico`) y los logotipos del paquete del menú contextual (`windows/assets`). Los ficheros de la marca anterior se conservan arriba, sin uso en los ejecutables.

### Paleta

| Color | Uso |
|---|---|
| `#0066FF` | Azul principal: confianza, tecnología, rendimiento |
| `#3B9CFF` | Azul secundario: movimiento, claridad |
| `#FF8A3D` | Naranja de acento: energía, progreso |
| `#0E1628` | Texto principal |
| `#8B9BB3` | Gris de interfaz |
| `#F4F7FF` | Fondo |

Tipografía: **Sora** para títulos y botones, **Inter** para el texto. Las dos son OFL; su licencia está en `arca-setup/assets/fonts`.

### Geometría del símbolo

Lienzo de 220 × 139. Las cuatro aristas largas comparten la misma pendiente (0,59), así que las patas conservan un grosor constante de pie a cima.

| Pieza | Medidas |
|---|---|
| Cima plana | x 82–138, y 0, esquinas de radio 5 |
| Pata izquierda (encima) | de la arista exterior (82, 0)–(0, 139) a la interior que pasa por (110, 45) |
| Pata derecha (debajo) | espejo de la izquierda |
| Vértice del hueco | (110, 45) |
| Triángulo naranja | vértice (110, 83), base y 139 de x 77 a 143 |
| Facetas de los pies | triángulos claros desde y 90 hasta la base |

Palabra: altura de mayúscula 41, trazo 10, cuatro letras en x 0, 82, 147 y 197 (ancho 262). La «A» no lleva travesaño. El eslogan «Archive engineering.» va en contornos de Sora Regular, justificado al ancho de la palabra; no depende de que la fuente esté instalada.

### Ficheros

| Fichero | Para qué |
|---|---|
| `arca-monolito.svg` | Símbolo solo |
| `arca-monolito-logotipo.svg` | Símbolo, palabra y eslogan en vertical, texto en `#0E1628` |
| `arca-monolito-logotipo-blanco.svg` | Igual, para fondos oscuros |
| `arca-monolito-horizontal.svg` | Símbolo a la izquierda, palabra y eslogan a la derecha |
| `arca-monolito-app-claro.svg` | Icono de aplicación sobre baldosa clara |
| `arca-monolito-app-oscuro.svg` | Icono de aplicación sobre baldosa oscura |
| `arca-monolito-simbolo.svg` | Símbolo simplificado, la «A» de la palabra; usa `currentColor` |
| `arca-monolito.ico` | 16, 20, 24, 32, 40, 48, 64, 128 y 256 px, del icono claro |
| `arca-monolito-256.png` | El mismo icono a 256 px |
