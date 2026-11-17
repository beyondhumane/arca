---
description: AES-256 en ZIP y 7z, nombres ocultos, contraseñas y límites de integridad.
group: Uso de Arca
order: 7
keywords: contraseña aes aes-256 cifrar descifrar zipcrypto hmac pbkdf2 seguridad password aes aes-256 encrypt decrypt zipcrypto hmac pbkdf2 security
---

# Cifrado

Arca cifra archivos `.zip` con AES-256 usando el esquema WinZip AE-2, el mismo que escriben 7-Zip, WinRAR y NanaZip. `interop.sh` lo comprueba en ambas direcciones contra 7-Zip.

## El esquema ZIP

| Paso | Algoritmo |
| --- | --- |
| Derivación de clave | PBKDF2-HMAC-SHA1, 1000 rondas |
| Cifrador | AES-256 en modo CTR |
| Autenticación | HMAC-SHA1 sobre el texto cifrado |
| Sal | 16 bytes aleatorios, nuevos en cada entrada |

### AES-128 y AES-192

WinZip AES también existe en 128 y 192 bits, con una sal de 8 o 12 bytes y una clave de 16 o 24 bytes. Arca lee ambas variantes con la misma comprobación de contraseña y el mismo veredicto HMAC que AES-256, así que una contraseña incorrecta o un byte alterado fallan igual. Nunca las escribe: `arca password` con una contraseña nueva pasa ese archivo a AES-256, mientras que añadir, quitar o renombrar entradas las copia con la fuerza que tenían.

## Decisiones de diseño

- **Una sal por entrada.** Reutilizar una sal reutilizaría el flujo de clave, y dos archivos idénticos se verían idénticos dentro del archivo comprimido.
- **Comprimir y luego cifrar.** Es el orden que pide la especificación, porque los bytes cifrados no dejan nada que comprimir al compresor.
- **El CRC se guarda como cero.** AE-2 lo exige: es una suma de verificación del texto plano y no tiene sentido una vez que el HMAC responde por los datos.
- **La manipulación falla de forma ruidosa.** Un byte alterado no sale como contenido; falla el código de autenticación.

> [!WARNING]
> **Descifrado en flujo**
>
> El descifrado funciona en flujo, así que el veredicto del HMAC llega cuando los bytes ya se han escrito. Si la extracción falla, descarta lo que se haya escrito.

> [!NOTE]
> **Los nombres de archivo son visibles**
>
> El formato ZIP no permite cifrar los nombres de archivo, así que cualquiera puede listar un archivo cifrado sin la contraseña.

## Uso

```sh
arca create secret.zip folder/ -p "a password"
arca extract secret.zip -o where/ -p "a password"
7z t -p"a password" secret.zip        # 7-Zip lo lee
```

## Cifrado 7z y nombres ocultos

```sh
arca create secret.7z folder/ -p "a password" --hide-names
arca list secret.7z -p "a password"
arca test secret.7z -p "a password"
arca extract secret.7z -o where/ -p "a password"
7z t -p"a password" secret.7z
```

7z usa AES-256-CBC y una KDF SHA-256 (potencia 19 al crear con Arca), con sal e IV
aleatorios nuevos para cada bloque de datos y cabecera. No es WinZip AE-2 y no
tiene HMAC: CRC-32 detecta corrupción accidental pero **no es autenticación
criptográfica**. Un fallo de descifrado/CRC significa contraseña incorrecta **o
archivo corrupto**, no demuestra cuál de los dos ha ocurrido.

Sin `--hide-names` los nombres son visibles y `list` no exige contraseña. Con
esa opción se cifran las cabeceras y hasta el listado la necesita. Arca valida
todos los datos cifrados antes de crear directorios, abrir destinos o sustituir
ficheros existentes. Después vuelve a descodificar lo seleccionado; el origen
debe permanecer estable entre ambas pasadas. Esto protege el destino ante fallos
de contraseña/CRC, no frente a una falsificación deliberada del CRC.

La contraseña no puede estar vacía. Si todas las entradas están vacías, hay que
ocultar los nombres para poder comprobarla. Las contraseñas CLI pueden aparecer
en el historial del shell y los argumentos del proceso; usa el campo de la
ventana si esto importa. No se garantiza el borrado seguro de secretos en memoria.

## ZipCrypto heredado

ZipCrypto, el esquema de contraseña antiguo, se lee pero nunca se escribe: un archivo de otra herramienta se abre con su contraseña, y `arca password` lo pasa a AES-256. El esquema está roto por diseño, así que no se crea nada nuevo con él. Su comprobación de contraseña es un único byte, así que una contraseña incorrecta de cada 256 lo supera y falla en la suma de verificación en su lugar.

## Cambiar la contraseña de un archivo existente

```sh
arca password secret.zip -p "a password"                 # la quita
arca password plain.zip --new "a password"               # pone una
arca password secret.zip -p "old" --new "new"            # la cambia
arca password secret.zip -p "a password" -o clean.zip    # deja el original intacto
```

**Nada se vuelve a comprimir.** AES cifra los bytes ya comprimidos, así que quitar el cifrado devuelve exactamente el flujo Deflate que había, y el tamaño comprimido no cambia. Lo que sí cuesta es el CRC: una entrada AE-2 guarda cero ahí, así que la entrada se descomprime una vez para calcularlo antes de poder escribirse sin cifrar.

> [!TIP]
> **Seguro en el sitio**
>
> Sustituir en el sitio destruiría la única copia de los datos, así que el nuevo archivo se construye junto al antiguo, se vuelve a leer entero para comprobar que es correcto, y solo entonces se mueve sobre él. Si algo falla, el original queda como estaba y no se deja ningún archivo temporal.

### En la ventana

El diálogo de creación admite contraseñas ZIP y 7z y **Ocultar nombres** en 7z.
Las cabeceras ocultas piden la contraseña antes del listado; el resto se valida
en segundo plano antes de extraer, probar, previsualizar o abrir una entrada.
Reintentar retoma la acción original; cancelar no escribe salida.

Cambiar una contraseña existente es **exclusivo de ZIP**. Añadir, borrar y
renombrar entradas también funciona en 7z: el archivo se descomprime y se vuelve
a comprimir con la misma contraseña, y con los nombres otra vez ocultos si lo
estaban.

## Contraseñas guardadas

La ventana puede guardar contraseñas de archivos en **Ajustes > Contraseñas** y probarlas antes de preguntar. La línea de órdenes todavía no las usa; `arca` sigue pidiendo `-p`.

### Qué se prueba, y cuándo

Cada contraseña guardada tiene una etiqueta, una máscara y un interruptor **Probar sin preguntar**. Una máscara es un patrón de nombre con `*` y `?` (`*.zip`, `copia-*.7z`), una carpeta (`D:\Copias\` abarca todo lo que hay debajo), varias de esas separadas por `;`, o nada, que encaja con cualquier archivo. No distingue mayúsculas de minúsculas.

Cuando abrir, extraer, probar, previsualizar, calcular sumas, comparar o editar un 7z cifrado necesita una contraseña, la ventana prueba primero las guardadas cuya máscara encaja y que tienen **Probar sin preguntar** activado: primero un nombre exacto, luego las máscaras con más caracteres fijos y al final las que no tienen máscara. Cada intento es la misma comprobación local que hace la ventana de contraseña; ninguna contraseña sale del ordenador. Las guardadas con el interruptor apagado no se prueban: la ventana de contraseña las ofrece como botones **Usar …**. Si ninguna sirve, la ventana pregunta como siempre, sin llamar contraseña incorrecta al intento.

**Guardar esta contraseña**, en la ventana de contraseña, guarda la que escribes, pero solo cuando ya ha abierto el archivo, así que una errata nunca se guarda. Se guarda con el nombre del archivo como máscara y **Probar sin preguntar** activado, y desde entonces ese archivo se abre sin preguntar. Amplía la máscara en Ajustes para abarcar una carpeta entera o una familia de nombres.

### La contraseña maestra

Sin contraseña maestra no se escribe nada en disco: las contraseñas guardadas duran hasta que se cierra Arca. Con ella, van a `passwords.vault`, junto a `gui.conf` (`%APPDATA%\Arca` en Windows, `~/Library/Application Support/Arca` en macOS, `$XDG_CONFIG_HOME/Arca` o `~/.config/Arca` en el resto). Nunca van a `gui.conf`, a un registro ni al portapapeles.

| Paso | Algoritmo |
| --- | --- |
| Derivación de clave | Argon2id (v1.3), 64 MiB, 3 pasadas, 1 carril, clave de 32 bytes |
| Sal | 16 bytes aleatorios, nuevos con cada contraseña maestra |
| Cifrado | AES-256-GCM, un nonce aleatorio de 12 bytes en cada escritura |
| Autenticación | La etiqueta GCM cubre la lista entera y, como datos asociados, la cabecera con los parámetros de coste |

El archivo empieza por `ARCAVLT\0`, una versión de formato y el coste de Argon2id, así que una versión posterior puede subir el coste o cambiar el formato y seguir leyendo este. Un archivo de un Arca más nuevo se rechaza en vez de leerse mal, y uno que pide un coste absurdo (más de 1 GiB, 64 pasadas o 16 carriles) se rechaza antes de hacer ningún trabajo.

- **Contraseña maestra incorrecta o archivo dañado.** GCM no los distingue, y Arca no finge hacerlo. Un archivo truncado o alterado se rechaza igual, nunca se lee a medias.
- **Nunca se sobrescribe a ciegas.** Antes de cada escritura Arca comprueba que el archivo sigue siendo el que leyó. Si ha cambiado (otra ventana, una restauración, un daño) se deja como está y el cambio se rechaza. Un archivo que no se puede abrir solo se puede **apartar**: se renombra con una marca de tiempo, no se borra, así que sigue abriéndose con la contraseña maestra antigua.
- **Bloqueo.** El almacén se bloquea tras 10 minutos sin uso y al cerrar la ventana. Bloquear descarta la clave y las contraseñas leídas del archivo; las guardadas solo para la sesión siguen hasta que se cierra Arca.
- **Memoria.** La clave, la contraseña maestra, la lista descifrada y las contraseñas guardadas viven en búferes que se ponen a cero al liberarse. Una contraseña que se pasa a un trabajo de abrir, extraer o probar sigue siendo una cadena normal, como una escrita a mano.

> [!NOTE]
> **ZipCrypto y los intentos automáticos**
>
> La comprobación de contraseña de ZipCrypto es un solo byte, así que una contraseña guardada incorrecta tiene una probabilidad entre 256 de superarla, y cada contraseña guardada que se prueba es una oportunidad más. Esa contraseña la detecta después la suma de comprobación al extraer, y la tarea falla en lugar de volver a preguntar. Los archivos AES comprueban un verificador de dos bytes y el HMAC, así que ahí es mucho menos probable.
