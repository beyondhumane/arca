---
description: AES-256 con WinZip AE-2: cómo funciona, qué garantiza y qué no.
group: Uso de Arca
order: 7
keywords: contraseña aes aes-256 cifrar descifrar zipcrypto hmac pbkdf2 seguridad password aes aes-256 encrypt decrypt zipcrypto hmac pbkdf2 security
---

# Cifrado

Arca cifra archivos `.zip` con AES-256 usando el esquema WinZip AE-2, el mismo que escriben 7-Zip, WinRAR y NanaZip. `interop.sh` lo comprueba en ambas direcciones contra 7-Zip.

## El esquema

| Paso | Algoritmo |
| --- | --- |
| Derivación de clave | PBKDF2-HMAC-SHA1, 1000 rondas |
| Cifrador | AES-256 en modo CTR |
| Autenticación | HMAC-SHA1 sobre el texto cifrado |
| Sal | 16 bytes aleatorios, nuevos en cada entrada |

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

El diálogo de creación tiene un campo de contraseña, y abrir un archivo cifrado pide la contraseña antes de extraer. La barra de herramientas muestra **Quitar contraseña** cuando el archivo abierto está cifrado y **Poner contraseña…** cuando no lo está.
