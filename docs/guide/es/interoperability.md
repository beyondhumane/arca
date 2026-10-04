---
description: Qué verifica interop.sh frente a unzip, tar y 7-Zip.
group: Referencia
order: 13
keywords: interoperabilidad compatibilidad 7-zip unzip tar winrar nanazip sha-256 interop compatibility 7-zip unzip tar winrar nanazip sha-256
---

# Interoperabilidad

Un archivador solo es útil si otras herramientas pueden abrir lo que escribe.
`interop.sh` compara hashes tras recorridos de ida y vuelta y comprueba rechazos
y limpieza. El resumen final da el número según la compilación y las herramientas.

## Qué comprueba interop.sh

- Lo que escribe Arca lo lee `unzip`, `tar` y 7-Zip, en los cuatro niveles.
- Lo que escriben `zip`, `tar` y 7-Zip lo lee Arca sin perder un byte.
- Un archivo que Arca cifró con AES-256 se abre en 7-Zip, y al revés.
- Añadir y quitar la contraseña de un archivo ZIP existente, sea de Arca o de 7-Zip.
- Un byte ZIP alterado lo detecta el CRC o el HMAC de AE-2 si está cifrado.
- 7z Copy/LZMA2 en todos los niveles, contraseñas y cabeceras ocultas en ambas direcciones.
- Entradas externas sólidas LZMA/LZMA2, nombres Unicode, entradas vacías y conflictos.
- Extraer 7z con contraseña incorrecta deja el destino ausente o intacto.
- Una entrada con `../../` se rechaza en lugar de escribir fuera del destino.
- `.tar.xz` en los cuatro niveles y con uno o cuatro hilos se abre con `xz -t` y `tar -J`, y Arca lee lo que escribe `tar -cJf`, también con el nombre `.txz`.
- Un `.xz` suelto va y vuelve con `xz`, con todos los tipos de `--check` y con flujos concatenados. Dos entradas se rechazan.
- Un TAR dentro de un fichero llamado `.xz` se lista como TAR.XZ; un `.xz` dañado falla sin publicar nada, y un `.tar.xz` truncado falla.

```sh
bash interop.sh
```

## Zstandard en ZIP

Zstandard es el método 93 de ZIP: registrado en la especificación, pero todavía no lo lee el `unzip` clásico. Por eso `-c auto` usa Deflate en un `.zip`, para que cualquier cosa pueda abrirlo. Zstandard se pide a mano, y será el predeterminado en cuanto exista un formato nativo.

## Cifrado

ZIP AES-256 usa WinZip AE-2. Los ZIP ZipCrypto se pueden actualizar con
`arca password`. 7z usa AES-256-CBC/SHA-256 y CRC, no cifrado autenticado: un fallo
de CRC cifrado también puede indicar corrupción.

```sh
7z t -p"a password" secret.zip        # 7-Zip lee lo que Arca cifró
```

## Regresiones del parser

```sh
cargo test -p arca-7z
cargo test -p arca-cli --test sevenz
cargo test -p arca-xz
cargo test -p arca-cli --test xz
cargo test -p arca-7z --test interop -- --ignored
cargo test -p arca-gui -- --ignored
```

Las pruebas ignoradas requieren `7z`. Las suites recorren todos los prefijos
truncados de ejemplos sin cifrar, sólidos, cifrados y con cabeceras ocultas y
exigen errores, no panics. También prueban cabeceras malformadas y límites de
recursos. Son regresiones, no una garantía sobre todo archivo malicioso.
No se publican nuevas cifras de rendimiento 7z. Las pruebas de plataforma e
interfaz deben realizarse por separado.
