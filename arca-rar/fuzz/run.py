"""Deterministic, process-isolated mutation fuzzing; no coverage-guided claim."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import random
import resource
import struct
import subprocess
import tempfile
import time
import zlib


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (1024**3, 1024**3))
    resource.setrlimit(resource.RLIMIT_CPU, (2, 2))
    resource.setrlimit(resource.RLIMIT_FSIZE, (2 * 1024**2, 2 * 1024**2))
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def unpack(data):
    frames, at = [], 2
    while at < len(data):
        length = struct.unpack_from("<I", data, at)[0]
        frames.append(bytearray(data[at + 4:at + 4 + length]))
        at += 4 + length
    return frames


def repair_rar5_crcs(data):
    if data[:8] != b"Rar!\x1a\x07\x01\x00":
        return

    def vint(at):
        value = 0
        for shift in range(0, 63, 7):
            byte = data[at]
            at += 1
            value |= (byte & 127) << shift
            if not byte & 128:
                return value, at
        raise ValueError("vint")

    at = 8
    try:
        while at + 5 <= len(data):
            length, start = vint(at + 4)
            end = start + length
            if end > len(data):
                return
            _, cursor = vint(start)
            flags, cursor = vint(cursor)
            if flags & 1:
                _, cursor = vint(cursor)
            packed = vint(cursor)[0] if flags & 2 else 0
            data[at:at + 4] = struct.pack("<I", zlib.crc32(data[at + 4:end]))
            at = end + packed
    except (IndexError, ValueError):
        pass


def mutate(seed, rng, iteration):
    frames = unpack(seed)
    frame = rng.choice(frames)
    operation = iteration % 8
    if operation in (0, 1, 2):
        if frame:
            for _ in range(1 if operation == 0 else 4):
                at = rng.randrange(len(frame))
                frame[at] ^= 1 << rng.randrange(8)
            if operation == 2:
                repair_rar5_crcs(frame)
    elif operation == 3:
        del frame[rng.randrange(len(frame) + 1):]
    elif operation == 4:
        frames.pop(rng.randrange(len(frames)))
    elif operation == 5 and len(frames) < 4:
        frames.append(bytearray(frame))
    elif operation == 6:
        at = rng.randrange(len(frame) + 1)
        frame[at:at] = bytes(rng.randrange(256) for _ in range(rng.randrange(1, 17)))
    mode = seed[0] if iteration % 3 else rng.randrange(64)
    password = seed[1] if iteration % 5 else rng.randrange(4)
    return bytes([mode, password]) + b"".join(struct.pack("<I", len(f)) + f for f in frames)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runs", type=int, default=2000)
    parser.add_argument("--seed", type=int, default=140014)
    parser.add_argument("--seconds", type=int, default=300)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    assert 0 <= args.runs <= 100000 and 1 <= args.seconds <= 3600
    root = Path(__file__).resolve().parent
    binary = root / "target/debug/arca-rar-bounded-fuzz"
    sources = sorted((root / "corpus").glob("*.case"))
    seeds = [p.read_bytes() for p in sources]
    manifest = json.loads((root / "corpus/manifest.json").read_text())
    for p, data in zip(sources, seeds):
        assert hashlib.sha256(data).hexdigest() == manifest["seeds"][p.name]["sha256"]
    args.output.mkdir(parents=True, exist_ok=True)
    rng, counts = random.Random(args.seed), Counter()
    started = time.monotonic()
    digest = hashlib.sha256()
    failures = []
    baseline = {}
    with tempfile.TemporaryDirectory(prefix="arca-fuzz-") as temporary:
        case = Path(temporary) / "input.case"
        for iteration in range(len(seeds) + args.runs):
            if time.monotonic() - started >= args.seconds:
                break
            data = seeds[iteration] if iteration < len(seeds) else mutate(rng.choice(seeds), rng, iteration - len(seeds))
            digest.update(struct.pack("<I", len(data)) + data)
            case.write_bytes(data)
            try:
                result = subprocess.run([str(binary), str(case)], capture_output=True, timeout=3,
                                        preexec_fn=limits, env={**os.environ, "TMPDIR": temporary})
                failed = result.returncode != 0
                output = (result.stdout + result.stderr).decode(errors="replace")
                counts.update(result.stdout.decode(errors="replace").splitlines())
                if iteration < len(seeds):
                    baseline[sources[iteration].name] = result.stdout.decode(errors="replace").splitlines()
                    failed = failed or "extracted" not in baseline[sources[iteration].name]
            except subprocess.TimeoutExpired as error:
                failed, output = True, str(error)
            counts["executions"] += 1
            if failed:
                name = f"failure-{iteration}.case"
                (args.output / name).write_bytes(data)
                (args.output / (name + ".txt")).write_text(output)
                failures.append(name)
                break
    report = {"seed": args.seed, "requested_mutations": args.runs, "wall_budget_seconds": args.seconds,
              "elapsed_seconds": round(time.monotonic() - started, 3), "counts": dict(counts),
              "input_stream_sha256": digest.hexdigest(), "failures": failures, "baseline": baseline,
              "per_case": {"wall_seconds": 3, "cpu_seconds": 2, "address_space_mib": 1024,
                           "file_size_mib": 2, "adapter_phase_cancel_ms": 250,
                           "input_kib": 128, "volumes": 4, "entries": 128, "declared_output_kib": 256}}
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    if failures:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
