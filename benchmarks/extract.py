#!/usr/bin/env python3
"""Reproducible external-process ZIP extraction benchmark (standard library only).

See README.md in this directory. All hashing, verification and cleanup is outside
the timer. This is a warm-cache benchmark, not a durability or cold-disk test.
"""

import argparse
import csv
import hashlib
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import tarfile
import time
import uuid
import zipfile
import zlib
from pathlib import Path


def sha(path):
    with open(path, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def capture(command):
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=90)
        return {"command": command, "returncode": result.returncode,
                "stdout": result.stdout, "stderr": result.stderr}
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"command": command, "error": str(error)}


def environment():
    data = {"platform": platform.platform(), "python": sys.version,
            "cpu_count": os.cpu_count(), "zlib": zlib.ZLIB_RUNTIME_VERSION,
            "rustc": capture(["rustc", "-Vv"]), "cargo": capture(["cargo", "-V"]),
            "git": capture(["git", "rev-parse", "HEAD"]),
            "environment": {key: os.environ.get(key) for key in
                            ("RAYON_NUM_THREADS", "RUSTFLAGS", "CARGO_PROFILE_RELEASE_LTO",
                             "CARGO_PROFILE_RELEASE_CODEGEN_UNITS")}}
    if os.name == "nt":
        script = """
        $ErrorActionPreference = 'Continue'
        @{
          os = Get-CimInstance Win32_OperatingSystem | Select Caption,Version,BuildNumber
          cpu = Get-CimInstance Win32_Processor | Select Name,NumberOfCores,NumberOfLogicalProcessors
          memory = Get-CimInstance Win32_ComputerSystem | Select TotalPhysicalMemory,Manufacturer,Model
          volumes = Get-Volume | Select DriveLetter,FileSystem,Size,SizeRemaining
          disks = Get-CimInstance Win32_DiskDrive | Select Model,InterfaceType,Size
          defender = Get-MpComputerStatus | Select AntivirusEnabled,RealTimeProtectionEnabled,AMProductVersion,AntivirusSignatureVersion
        } | ConvertTo-Json -Depth 5
        """
        data["windows"] = capture(["powershell", "-NoProfile", "-Command", script])
    else:
        data["disk"] = capture(["df", "-T", "."])
        data["cpu"] = capture(["lscpu"])
    return data


def generated_entries(count, nested, large=False):
    for i in range(count):
        size = 16 * 1024 * 1024 if large else 512 + (i * 7919) % (16384 - 512 + 1)
        seed = f"arca-extraction-v1:{i}".encode("ascii")
        noise = hashlib.shake_256(seed).digest(size // 2)
        line = b"function example() { return 'deterministic source-like data'; }\n"
        body = (line * ((size - len(noise)) // len(line) + 1))[:size - len(noise)] + noise
        parent = f"d{i // 100:04}/s{i % 10:02}/" if nested else ""
        yield f"{parent}f{i:06}.txt", body


def generate(root, counts):
    root.mkdir(parents=True, exist_ok=True)
    cases = [(f"small-{count}-{layout}", count, layout == "nested", False)
             for count in counts for layout in ("flat", "nested")]
    cases.append(("large-8", 8, False, True))
    for name, count, nested, large in cases:
        archive = root / (name + ".zip")
        manifest_path = root / (name + ".json")
        if archive.exists() or manifest_path.exists():
            raise ValueError(f"Refusing to replace corpus: {archive}")
        entries = []
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as out:
            for path, body in generated_entries(count, nested, large):
                info = zipfile.ZipInfo(path, date_time=(2020, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.create_system = 3
                info.external_attr = 0o100644 << 16
                out.writestr(info, body, compresslevel=6)
                entries.append({"path": path, "size": len(body),
                                "sha256": hashlib.sha256(body).hexdigest()})
            info = zipfile.ZipInfo("empty/", date_time=(2020, 1, 1, 0, 0, 0))
            info.external_attr = (0o40755 << 16) | 0x10
            out.writestr(info, b"")
        manifest = {"generator": "arca-extraction-v1", "case": name,
                    "files": count, "bytes": sum(e["size"] for e in entries),
                    "archive_sha256": sha(archive), "archive_bytes": archive.stat().st_size,
                    "empty_dirs": ["empty"], "entries": entries,
                    "python": sys.version, "zlib": zlib.ZLIB_RUNTIME_VERSION}
        save(manifest_path, manifest)
        print(name, manifest["files"], manifest["bytes"], manifest["archive_sha256"], flush=True)


def verify(dest, manifest):
    expected = {e["path"] for e in manifest["entries"]}
    found = {p.relative_to(dest).as_posix() for p in dest.rglob("*") if p.is_file()}
    if found != expected:
        raise ValueError(f"Output paths differ: missing={len(expected-found)}, extra={len(found-expected)}")
    for entry in manifest["entries"]:
        path = dest / entry["path"]
        if path.stat().st_size != entry["size"] or sha(path) != entry["sha256"]:
            raise ValueError(f"Output content differs: {path}")
    for directory in manifest["empty_dirs"]:
        if not (dest / directory).is_dir():
            raise ValueError(f"Missing empty directory: {directory}")


def source_corpus(args):
    if sha(args.tar) != args.sha256:
        raise ValueError("Source tarball hash differs")
    args.corpus.mkdir(parents=True, exist_ok=True)
    archive = args.corpus / (args.name + ".zip")
    entries = []
    prefix = args.prefix.rstrip("/") + "/"
    with tarfile.open(args.tar, "r:*") as source, zipfile.ZipFile(archive, "x") as out:
        for member in sorted(source.getmembers(), key=lambda m: m.name):
            if not member.isfile() or not member.name.startswith(prefix):
                continue
            name = member.name[len(prefix):]
            if not name or name.startswith("/") or ".." in name.split("/") or "\\" in name:
                raise ValueError("Unsafe source member name")
            with source.extractfile(member) as stream:
                body = stream.read()
            info = zipfile.ZipInfo(name, date_time=(2020, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            out.writestr(info, body, compresslevel=6)
            entries.append({"path": name, "size": len(body),
                            "sha256": hashlib.sha256(body).hexdigest()})
    if not entries:
        raise ValueError("No regular files matched the source prefix")
    manifest = {"generator": "source-tar-v1", "case": args.name,
                "source_tar_sha256": args.sha256, "source_prefix": args.prefix,
                "files": len(entries), "bytes": sum(e["size"] for e in entries),
                "archive_sha256": sha(archive), "archive_bytes": archive.stat().st_size,
                "empty_dirs": [], "entries": entries,
                "python": sys.version, "zlib": zlib.ZLIB_RUNTIME_VERSION}
    save(args.corpus / (args.name + ".json"), manifest)
    print(args.name, manifest["files"], manifest["bytes"], manifest["archive_sha256"])


def summarize(rows):
    result = []
    groups = sorted({(r["case"], r["variant"]) for r in rows if not r["warmup"]})
    for case, variant in groups:
        values = [r["seconds"] for r in rows if not r["warmup"]
                  and r["case"] == case and r["variant"] == variant and r["verified"]]
        if not values:
            continue
        median = statistics.median(values)
        q1, _, q3 = statistics.quantiles(values, n=4, method="inclusive") if len(values) > 1 else [median] * 3
        result.append({"case": case, "variant": variant, "n": len(values),
                       "median_s": median, "min_s": min(values), "max_s": max(values),
                       "iqr_s": q3-q1, "mad_s": statistics.median(abs(v-median) for v in values)})
    return result


def run(args):
    variants = json.loads(args.variants.read_text(encoding="utf-8"))
    if len({v["name"] for v in variants}) != len(variants) or not variants:
        raise ValueError("Variant names must be unique and nonempty")
    args.results.mkdir(parents=True, exist_ok=False)
    args.scratch.mkdir(parents=True, exist_ok=True)
    metadata = environment()
    metadata.update({"arguments": vars(args) | {"func": None},
                     "cache_policy": "Warmups; no cache flush. Verification and cleanup between runs.",
                     "timing": "perf_counter_ns around subprocess.run, including process start/exit; no fsync",
                     "variants": []})
    metadata["arguments"] = {k: str(v) for k, v in metadata["arguments"].items()}
    for variant in variants:
        exe = Path(variant["command"][0]).resolve()
        variant["command"][0] = str(exe)
        metadata["variants"].append(variant | {"binary_sha256": sha(exe),
            "version": capture([str(exe), "i" if variant.get("kind") == "7zip" else "--version"])})
    save(args.results / "environment.json", metadata)
    rows = []
    with open(args.results / "samples.jsonl", "w", encoding="utf-8") as log:
        for case in args.cases:
            archive = (args.corpus / (case + ".zip")).resolve()
            manifest_path = args.corpus / (case + ".json")
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            if sha(archive) != manifest["archive_sha256"]:
                raise ValueError(f"Archive hash differs: {archive}")
            shutil.copyfile(manifest_path, args.results / manifest_path.name)
            # Forward/reverse pairs balance ordering and adjacency. Rotate pairs
            # so no variant is always first in warmups or measured runs.
            for run_index in range(-args.warmups, args.runs):
                pair = max(run_index, 0) // 2
                order = variants[pair % len(variants):] + variants[:pair % len(variants)]
                if run_index % 2:
                    order = list(reversed(order))
                for position, variant in enumerate(order):
                    dest = (args.scratch / ("run-" + uuid.uuid4().hex)).resolve()
                    command = [part.replace("{archive}", str(archive)).replace("{dest}", str(dest))
                               for part in variant["command"]]
                    started = time.time()
                    start = time.perf_counter_ns()
                    output = subprocess.run(command, capture_output=True, timeout=args.timeout)
                    seconds = (time.perf_counter_ns() - start) / 1e9
                    row = {"case": case, "variant": variant["name"], "run": run_index,
                           "warmup": run_index < 0, "order": position, "started_unix": started,
                           "seconds": seconds, "returncode": output.returncode, "command": command,
                           "stdout": output.stdout.decode(errors="replace"),
                           "stderr": output.stderr.decode(errors="replace"), "verified": False}
                    try:
                        if output.returncode:
                            raise ValueError(f"Extractor failed: {row['stderr']}")
                        verify(dest, manifest)
                        row["verified"] = True
                    finally:
                        log.write(json.dumps(row) + "\n")
                        log.flush()
                        rows.append(row)
                    shutil.rmtree(dest)
                    print(case, variant["name"], run_index, f"{seconds:.6f}s", flush=True)
            save(args.results / "summary.json", summarize(rows))
    fields = ["case", "variant", "run", "warmup", "order", "started_unix", "seconds", "returncode", "verified"]
    with open(args.results / "samples.csv", "w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=fields, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    gen = commands.add_parser("generate")
    gen.add_argument("--corpus", type=Path, required=True)
    gen.add_argument("--counts", type=int, nargs="+", default=[5000, 20000, 50000])
    gen.set_defaults(func=lambda args: generate(args.corpus, args.counts))
    source = commands.add_parser("source")
    source.add_argument("--tar", type=Path, required=True)
    source.add_argument("--sha256", required=True)
    source.add_argument("--prefix", required=True)
    source.add_argument("--name", required=True)
    source.add_argument("--corpus", type=Path, required=True)
    source.set_defaults(func=source_corpus)
    bench = commands.add_parser("run")
    for option in ("corpus", "variants", "results", "scratch"):
        bench.add_argument("--" + option, type=Path, required=True)
    bench.add_argument("--cases", nargs="+", required=True)
    bench.add_argument("--runs", type=int, default=6)
    bench.add_argument("--warmups", type=int, default=1)
    bench.add_argument("--timeout", type=float, default=600)
    bench.set_defaults(func=run)
    args = parser.parse_args()
    if args.action == "run" and (args.runs < 5 or args.warmups < 1):
        parser.error("At least five measured runs and one warmup are required")
    args.func(args)


if __name__ == "__main__":
    main()
