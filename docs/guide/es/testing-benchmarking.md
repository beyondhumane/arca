---
description: Verifica archivos sin escribir en disco, y mide el rendimiento de forma reproducible.
group: Uso de Arca
order: 6
keywords: verificar crc ci github actions benchmark metodología test verify crc ci github actions bench benchmark r1 r2 methodology
---

# Pruebas y benchmarking

## arca test

Lee cada entrada y comprueba su CRC (o su HMAC, cuando la entrada está cifrada) sin escribir nada en disco.

```sh
arca test arca.zip
arca test secret.zip -p "a password"
```

```text
174 entries verified, no errors (0.006 s)
```

Las entradas corruptas se informan por la salida de error estándar, y el comando termina con el estado 1 y un recuento de las entradas que fallaron.

### Verificar artefactos en CI

```yaml
# with arca on the PATH; see Installation
- name: Package
  run: arca create dist.zip build/ -l best
- name: Verify
  run: arca test dist.zip
```

## arca bench

Mide los requisitos de rendimiento R1 y R2 del diseño frente a un archivo real y reporta PASS o FAIL.

```sh
arca bench big.zip
```

Imprime la cifra R2 del archivo que le des, y el comando `hyperfine` para R1:

```text
Performance requirements (design document, section 05)

  R2  list without extracting
      6000 entries in a 828.8 KB archive
      1.1 ms   target < 200 ms   PASS

  R1  cold start: open and list a one-entry archive, with hyperfine
      hyperfine -N --warmup 20 'arca list tiny.zip'
```

Las cifras publicadas de R1, R2 y R3, y el script que las reproduce, están en la página de [benchmarks](benchmarks.md).

## Benchmarks reproducibles

- Reporta el mejor de varias ejecuciones, y borra el directorio de salida antes de cada pasada.
- Los corpus pequeños quedan en la caché de escritura del sistema operativo y hacen que cualquier herramienta parezca más rápida. Usa datos que no quepan.
- Cuando una ejecución escribe gigabytes, alterna los brazos (A B B A) y mide el disco con una escritura secuencial simple antes y después.
- Compara el tiempo de CPU con el tiempo real. Si coinciden, el programa es su propio cuello de botella; si el tiempo real es mayor, lo es el disco.

> [!WARNING]
> **Alterna los brazos**
>
> Un barrido ordenado sobre el número de hilos mostró en una ocasión un hilo corriendo el doble de rápido que dieciséis. El disco se iba degradando a medida que avanzaba la ejecución, así que "más hilos" en realidad significaba "más tarde en la ejecución". Al repetir alternando, 16 hilos superaron a 1 hilo en las tres rondas.
