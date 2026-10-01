---
description: Qué verifica interop.sh frente a unzip, tar y 7-Zip.
group: Referencia
order: 13
keywords: interoperabilidad compatibilidad 7-zip unzip tar winrar nanazip sha-256 interop compatibility 7-zip unzip tar winrar nanazip sha-256
---

# Interoperabilidad

Un archivador solo es útil si otras herramientas pueden abrir lo que escribe. `interop.sh` comprueba 35 casos, verificando el SHA-256 del contenido cada vez.

## Qué comprueba interop.sh

- Lo que escribe Arca lo lee `unzip`, `tar` y 7-Zip, en los cuatro niveles.
- Lo que escriben `zip`, `tar` y 7-Zip lo lee Arca sin perder un byte.
- Un archivo que Arca cifró con AES-256 se abre en 7-Zip, y al revés.
- Añadir y quitar la contraseña de un archivo existente, sea de Arca o de 7-Zip.
- Un byte alterado lo detecta el CRC, o el HMAC cuando está cifrado.
- Una entrada con `../../` se rechaza en lugar de escribir fuera del destino.

```sh
bash interop.sh
```

## Zstandard en ZIP

Zstandard es el método 93 de ZIP: registrado en la especificación, pero todavía no lo lee el `unzip` clásico. Por eso `-c auto` usa Deflate en un `.zip`, para que cualquier cosa pueda abrirlo. Zstandard se pide a mano, y será el predeterminado en cuanto exista un formato nativo.

## Cifrado

Los archivos AES-256 usan WinZip AE-2, el esquema que escriben 7-Zip, WinRAR y NanaZip. Los archivos ZipCrypto heredados de otras herramientas se abren con su contraseña y se pueden actualizar con `arca password`.

```sh
7z t -p"a password" secret.zip        # 7-Zip lee lo que Arca cifró
```
