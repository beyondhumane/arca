---
description: Cómo compilar, probar y contribuir, y la licencia Apache-2.0.
group: Referencia
order: 16
keywords: contribuir pull request issue error licencia apache contribute pull request issue bug license apache
---

# Contribuir y licencia

Arca se desarrolla de forma abierta en GitHub. Los informes de errores, los benchmarks de tu propio hardware y los pull requests son bienvenidos.

## Configuración

```sh
git clone https://github.com/beyondhumane/arca
cd arca
cargo build --release
cargo test --workspace
bash interop.sh            # necesita zip, unzip, tar y 7-Zip
```

## Reglas básicas

- **Los parsers se mantienen seguros.** Nada que lea bytes de un archivo comprimido lleva código unsafe, y el forbid a nivel de crate lo convierte en un error de compilación de todos modos.
- **Las pruebas acompañan a las correcciones.** Cada error corregido añade una prueba de regresión.
- **interop.sh se mantiene en verde.** Lo que Arca escribe debe abrirse en otro sitio, y viceversa.
- **Los benchmarks vienen con su comando.** El mejor de varias ejecuciones, alternando los brazos, publicando la máquina y las pérdidas.

## Reportar un error

[Abre un issue](https://github.com/beyondhumane/arca/issues/new) e incluye `arca --version`, tu sistema operativo, el comando exacto que ejecutaste y qué pasó. Si puedes compartir el archivo, o uno más pequeño que reproduzca el problema, mejor todavía. Revisa primero los [issues existentes](https://github.com/beyondhumane/arca/issues).

## Licencia

Arca tiene licencia [Apache License 2.0](https://github.com/beyondhumane/arca/blob/main/LICENSE). Puedes usarla, modificarla y distribuirla, comercialmente o no, siempre que conserves la licencia y los avisos. La licencia incluye una concesión expresa de derechos de patente por parte de los contribuidores.
