"""Deterministic RAR creation stress and interoperability harness for the CLI.

Drives a built `arca` binary through create/list/test/extract cycles on seeded
fixtures, hashes every source tree and every independently extracted tree, and
cross-checks the archives with official UnRAR and 7-Zip when they are present.
Every child process runs under a wall-clock timeout and an address-space
ceiling. Results are written as JSON; on failure the archive, logs and a
listing of the source tree are kept under the output directory. A `warn`
result records a documented limitation that is not hidden by a passing exit
code; `skip` names a check that could not run here (for example no official
UnRAR). `--require-tools` turns tool skips into failures for CI.

Default mode is the bounded CI smoke subset. `--stress` enables the large
dataset, the 10k-entry archive and 100 create/list/test/extract cycles.
"""

import argparse
import hashlib
import json
import os
import platform
import random
import re
import resource
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FIXTURE_RAR5 = ROOT / "arca-rar" / "tests" / "fixtures" / "plain.rar"
LEVELS = ("store", "fast", "normal", "best")
MIB = 1024 * 1024
TEMP_PATTERN = re.compile(r"^\.arca-.*\.rar\.part$")
HARNESS_VERSION = 1


class Failure(Exception):
    pass


class Skip(Exception):
    pass


def limits(memory_bytes):
    def apply():
        resource.setrlimit(resource.RLIMIT_AS, (memory_bytes, memory_bytes))
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    return apply


class Run:
    def __init__(self, args, rc, stdout, stderr, seconds, max_rss_kib, timed_out):
        self.args = [str(a) for a in args]
        self.rc = rc
        self.stdout = stdout
        self.stderr = stderr
        self.seconds = seconds
        self.max_rss_kib = max_rss_kib
        self.timed_out = timed_out

    def text(self):
        return self.stdout + self.stderr

    def record(self):
        return {"args": self.args, "rc": self.rc, "seconds": round(self.seconds, 3),
                "max_rss_kib": self.max_rss_kib, "timed_out": self.timed_out,
                "stderr": self.stderr[-2000:]}


class Harness:
    def __init__(self, opts):
        self.opts = opts
        self.rng = random.Random(opts.seed)
        self.results = []
        self.work = Path(tempfile.mkdtemp(prefix="arca-create-stress.", dir=opts.workdir))
        self.out = Path(opts.output)
        self.out.mkdir(parents=True, exist_ok=True)
        self.artifacts = self.out / "artifacts"
        self.runs = []
        self.tools = {}

    # -- process control -------------------------------------------------
    def run(self, args, timeout=None, cwd=None, env=None, signal_after=None, sig=signal.SIGINT):
        timeout = timeout or self.opts.timeout
        memory = self.opts.memory_mib * MIB
        with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
            start = time.monotonic()
            proc = subprocess.Popen([str(a) for a in args], stdout=out, stderr=err, stdin=subprocess.DEVNULL,
                                    cwd=cwd, env=env, preexec_fn=limits(memory))
            timed_out = False
            signalled = False
            while True:
                pid, status, usage = os.wait4(proc.pid, os.WNOHANG)
                if pid:
                    break
                elapsed = time.monotonic() - start
                if signal_after is not None and not signalled and elapsed >= signal_after:
                    proc.send_signal(sig)
                    signalled = True
                if elapsed > timeout:
                    proc.kill()
                    pid, status, usage = os.wait4(proc.pid, 0)
                    timed_out = True
                    break
                time.sleep(0.005)
            seconds = time.monotonic() - start
            rc = os.waitstatus_to_exitcode(status) if hasattr(os, "waitstatus_to_exitcode") else status
            out.seek(0)
            err.seek(0)
            run = Run(args, rc, out.read().decode("utf-8", "replace"), err.read().decode("utf-8", "replace"),
                      seconds, usage.ru_maxrss, timed_out)
        self.runs.append(run)
        if timed_out:
            raise Failure(f"timed out after {timeout}s: {' '.join(run.args)}")
        return run

    def arca(self, *args, **kw):
        return self.run([self.opts.arca, *args], **kw)

    def expect_ok(self, run, what):
        if run.rc != 0:
            raise Failure(f"{what} failed with rc={run.rc}: {run.stderr.strip()[-400:]}")
        return run

    def expect_fail(self, run, what, needle=None):
        if run.rc == 0:
            raise Failure(f"{what} unexpectedly succeeded")
        if needle and needle not in run.text():
            raise Failure(f"{what}: expected message containing {needle!r}, got {run.text().strip()[-400:]!r}")
        return run

    # -- tools -----------------------------------------------------------
    def discover_tools(self):
        self.tools["arca"] = {"path": str(self.opts.arca), "version": self.arca("--version").stdout.strip()}
        self.tools["unrar"] = self.find_unrar()
        self.tools["sevenzip"] = self.find_sevenzip()

    def banner(self, path):
        try:
            run = self.run([path], timeout=30)
        except (OSError, Failure):
            return ""
        return run.text()

    def find_unrar(self):
        candidates = [self.opts.unrar, os.environ.get("UNRAR"), shutil.which("unrar")]
        for candidate in candidates:
            if not candidate:
                continue
            match = re.search(r"UNRAR (\d+\.\d+)", self.banner(candidate))
            if not match:
                continue
            probe = self.run([candidate, "t", "-inul", str(FIXTURE_RAR5)], timeout=60)
            if probe.rc == 0:
                return {"path": candidate, "version": f"UNRAR {match.group(1)}"}
        return None

    def find_sevenzip(self):
        candidates = [self.opts.sevenzip, os.environ.get("SEVENZIP"), shutil.which("7zz"), shutil.which("7z")]
        for candidate in candidates:
            if not candidate:
                continue
            match = re.search(r"7-Zip[^\n]*?(\d+\.\d+)", self.banner(candidate))
            if not match:
                continue
            # p7zip 16.02 lacks the RAR5 compressed-stream decoder; prove the
            # tool on a WinRAR-made compressed fixture before trusting it.
            probe = self.run([candidate, "t", "-bso0", "-bsp0", str(FIXTURE_RAR5)], timeout=60)
            if probe.rc == 0:
                return {"path": candidate, "version": f"7-Zip {match.group(1)}"}
        return None

    # -- fixtures --------------------------------------------------------
    def text_block(self, rng, size):
        words = ["archive", "arca", "rar", "entropy", "deterministic", "seed", "stream",
                 "block", "header", "member", "volume", "verify", "compress", "store"]
        parts = []
        total = 0
        while total < size:
            line = " ".join(rng.choice(words) for _ in range(rng.randint(3, 12))) + "\n"
            parts.append(line)
            total += len(line)
        return "".join(parts).encode()[:size]

    def payload(self, rng, kind, size):
        if size == 0:
            return b""
        if kind == "random":
            return rng.randbytes(size)
        if kind == "repetitive":
            unit = rng.randbytes(rng.randint(1, 16))
            return (unit * (size // len(unit) + 1))[:size]
        if kind == "zeros":
            return bytes(size)
        return self.text_block(rng, size)

    def write(self, path, data):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)

    def basic_tree(self, root, rng):
        self.write(root / "readme.txt", self.text_block(rng, 40 * 1024))
        self.write(root / "random.bin", rng.randbytes(96 * 1024))
        self.write(root / "repeat.dat", self.payload(rng, "repetitive", 64 * 1024))
        self.write(root / "zeros.bin", bytes(32 * 1024))
        self.write(root / "empty.txt", b"")
        self.write(root / "one.byte", b"\x00")
        (root / "empty-dir").mkdir()
        (root / "nested" / "a" / "b" / "c" / "d").mkdir(parents=True)
        self.write(root / "nested" / "a" / "b" / "c" / "d" / "deep.txt", self.text_block(rng, 1000))
        (root / "nested" / "a" / "also-empty").mkdir()
        self.write(root / "name with spaces.txt", b"spaces\n")
        self.write(root / "ni\u00f1o \u00e1\u00e9\u00ed\u00f3\u00fa.txt", b"accents\n")
        self.write(root / "\u65e5\u672c\u8a9e" / "\u30c6\u30b9\u30c8.txt", b"japanese\n")
        self.write(root / "\u0440\u0443\u0441\u0441\u043a\u0438\u0439.bin", rng.randbytes(777))
        self.write(root / "mixed" / "\u00e9moji-\U0001F4E6.txt", b"emoji\n")
        self.write(root / "dots.and.many.extensions.tar.gz.txt", b"dots\n")

    def compressible_tree(self, root, rng, size):
        self.write(root / "corpus.txt", self.text_block(rng, size))

    def many_tree(self, root, rng, count):
        for index in range(count):
            sub = root / f"d{index % 37:02d}"
            kind = ("text", "random", "repetitive", "empty")[index % 4]
            size = 0 if kind == "empty" else rng.randint(1, 300)
            self.write(sub / f"f{index:05d}.{kind}", self.payload(rng, kind, size))

    def large_tree(self, root, rng, total):
        # One long stream mixing repetitive, random and text blocks, written in
        # bounded chunks so the harness itself never holds the dataset in memory.
        root.mkdir(parents=True, exist_ok=True)
        written = 0
        with open(root / "stream.bin", "wb") as f:
            while written < total:
                size = min(MIB, total - written)
                kind = rng.choice(("repetitive", "random", "text", "zeros"))
                f.write(self.payload(rng, kind, size))
                written += size
        self.write(root / "tail.txt", self.text_block(rng, 4096))

    def cycle_tree(self, root, rng):
        count = rng.randint(1, 12)
        names = set()
        for _ in range(count):
            depth = rng.choice((0, 0, 1, 2, 3))
            parts = [rng.choice(("src", "docs", "img", "\u00f1", "deep", "x y")) for _ in range(depth)]
            stem = rng.choice(("a", "b", "readme", "data", "\u00fc", "caf\u00e9", "long-" + "x" * rng.randint(1, 40)))
            name = f"{stem}-{rng.randint(0, 9999)}.{rng.choice(('txt', 'bin', 'dat'))}"
            rel = "/".join(parts + [name])
            if rel.lower() in names:
                continue
            names.add(rel.lower())
            kind = rng.choice(("text", "random", "repetitive", "zeros", "empty"))
            size = 0 if kind == "empty" else rng.randint(1, 64 * 1024)
            self.write(root / rel, self.payload(rng, kind, size))
        if rng.random() < 0.3:
            (root / f"empty-{rng.randint(0, 99)}").mkdir(parents=True, exist_ok=True)
        if not any(root.iterdir()):
            self.write(root / "only.txt", b"only\n")

    # -- hashing ---------------------------------------------------------
    def tree_hash(self, root):
        lines = []
        files = 0
        dirs = 0
        total = 0
        for path in sorted(root.rglob("*")):
            rel = path.relative_to(root).as_posix()
            st = path.lstat()
            if stat.S_ISDIR(st.st_mode):
                lines.append(f"D {rel}")
                dirs += 1
            elif stat.S_ISREG(st.st_mode):
                h = hashlib.sha256()
                with open(path, "rb") as f:
                    for chunk in iter(lambda: f.read(MIB), b""):
                        h.update(chunk)
                lines.append(f"F {rel} {st.st_size} {h.hexdigest()}")
                files += 1
                total += st.st_size
            else:
                lines.append(f"? {rel}")
        digest = hashlib.sha256("\n".join(lines).encode()).hexdigest()
        return {"digest": digest, "files": files, "dirs": dirs, "bytes": total}

    def expect_same_tree(self, expected, actual_root, what):
        actual = self.tree_hash(actual_root)
        if actual["digest"] != expected["digest"]:
            raise Failure(f"{what}: extracted tree differs (expected {expected}, got {actual})")
        return actual

    def expect_clean_dir(self, directory, allowed):
        leftovers = sorted(p.name for p in directory.iterdir() if p.name not in allowed)
        temps = [n for n in leftovers if TEMP_PATTERN.match(n)]
        if temps:
            raise Failure(f"temporary files left in {directory}: {temps}")
        if leftovers:
            raise Failure(f"unexpected files left in {directory}: {leftovers}")

    # -- external extraction --------------------------------------------
    def unrar_check(self, archive, expected, into, archive_root):
        tool = self.tools.get("unrar")
        if not tool:
            return "skipped: no official UnRAR"
        self.expect_ok(self.run([tool["path"], "t", "-inul", str(archive)]), "unrar t")
        into.mkdir()
        self.expect_ok(self.run([tool["path"], "x", "-inul", "-y", str(archive), str(into) + os.sep]), "unrar x")
        self.expect_same_tree(expected, into / archive_root, "unrar x")
        return tool["version"]

    def sevenzip_check(self, archive, expected, into, archive_root):
        tool = self.tools.get("sevenzip")
        if not tool:
            return "skipped: no 7-Zip with RAR5 support"
        self.expect_ok(self.run([tool["path"], "t", "-bso0", "-bsp0", str(archive)]), "7z t")
        into.mkdir()
        self.expect_ok(self.run([tool["path"], "x", "-bso0", "-bsp0", "-y", f"-o{into}", str(archive)]), "7z x")
        self.expect_same_tree(expected, into / archive_root, "7z x")
        return tool["version"]

    def list_names(self, archive):
        run = self.expect_ok(self.arca("list", str(archive)), "arca list")
        names = []
        for line in run.stdout.splitlines():
            parts = line.split(None, 3)
            if len(parts) == 4:
                names.append(parts[3])
        return names

    def expected_names(self, root, input_name):
        names = []
        for path in sorted(root.rglob("*")):
            names.append(f"{input_name}/{path.relative_to(root).as_posix()}")
        return [input_name] + names

    def roundtrip(self, case, root, level, timeout=None, external=True):
        """create -> list -> test -> extract (arca, unrar, 7z); returns a record."""
        area = root.parent
        archive = area / f"{case}-{level}.rar"
        expected = self.tree_hash(root)
        create = self.expect_ok(self.arca("create", str(archive), str(root), "-l", level, timeout=timeout),
                                f"arca create -l {level}")
        if not archive.is_file():
            raise Failure("create reported success without an output file")
        self.expect_clean_dir(area, {p.name for p in area.iterdir() if p.suffix == ".rar" or p.is_dir()})
        names = self.list_names(archive)
        want = self.expected_names(root, root.name)
        if sorted(names) != sorted(want):
            missing = sorted(set(want) - set(names))[:5]
            extra = sorted(set(names) - set(want))[:5]
            raise Failure(f"listing mismatch: missing {missing}, extra {extra}")
        test = self.expect_ok(self.arca("test", str(archive), timeout=timeout), "arca test")
        dest = area / f"{case}-{level}-arca"
        dest.mkdir()
        extract = self.expect_ok(self.arca("extract", str(archive), "-o", str(dest), timeout=timeout), "arca extract")
        self.expect_same_tree(expected, dest / root.name, "arca extract")
        record = {
            "level": level,
            "source": expected,
            "archive_bytes": archive.stat().st_size,
            "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest() if archive.stat().st_size < 64 * MIB else None,
            "create": create.record(),
            "test_seconds": round(test.seconds, 3),
            "extract_seconds": round(extract.seconds, 3),
            "entries": len(names),
        }
        if external:
            record["unrar"] = self.unrar_check(archive, expected, area / f"{case}-{level}-unrar", root.name)
            record["sevenzip"] = self.sevenzip_check(archive, expected, area / f"{case}-{level}-7z", root.name)
        return record, archive

    # -- test driver -----------------------------------------------------
    def test(self, name, fn, **kw):
        area = self.work / name
        area.mkdir(parents=True)
        first_run = len(self.runs)
        start = time.monotonic()
        entry = {"name": name, "status": "pass"}
        try:
            detail = fn(area, **kw)
            entry["detail"] = detail
            if self.opts.require_tools and isinstance(detail, dict):
                skipped = [k for k, v in detail.items() if isinstance(v, str) and v.startswith("skipped")]
                if skipped:
                    entry["status"] = "fail"
                    entry["error"] = f"required external tools unavailable: {skipped}"
            if isinstance(detail, dict) and detail.get("warnings"):
                entry["status"] = "warn"
        except Skip as skip:
            entry["status"] = "skip"
            entry["reason"] = str(skip)
        except Failure as failure:
            entry["status"] = "fail"
            entry["error"] = str(failure)
        except Exception as error:  # harness bug: still recorded, never swallowed
            entry["status"] = "fail"
            entry["error"] = f"harness error {type(error).__name__}: {error}"
        entry["seconds"] = round(time.monotonic() - start, 3)
        runs = self.runs[first_run:]
        entry["processes"] = len(runs)
        entry["max_rss_kib"] = max((r.max_rss_kib for r in runs), default=0)
        if entry["status"] == "fail" or self.opts.keep:
            self.keep_artifacts(name, area, runs)
        if entry["status"] != "fail":
            shutil.rmtree(area, ignore_errors=True)
        self.results.append(entry)
        marker = {"pass": "ok  ", "fail": "FAIL", "skip": "skip", "warn": "WARN"}[entry["status"]]
        extra = entry.get("error") or entry.get("reason") or "; ".join((entry.get("detail") or {}).get("warnings", []) if isinstance(entry.get("detail"), dict) else [])
        print(f"  {marker} {name} ({entry['seconds']:.1f}s) {extra}".rstrip(), flush=True)

    def keep_artifacts(self, name, area, runs):
        target = self.artifacts / name
        target.mkdir(parents=True, exist_ok=True)
        try:
            for path in area.rglob("*"):
                if path.is_file() and (path.suffix == ".rar" or TEMP_PATTERN.match(path.name)) and path.stat().st_size <= 256 * MIB:
                    shutil.copy2(path, target / path.name)
        except OSError:
            pass
        listing = []
        try:
            for path in sorted(area.rglob("*")):
                st = path.lstat()
                listing.append(f"{stat.filemode(st.st_mode)} {st.st_size:>12} {path.relative_to(area).as_posix()}")
        except OSError as error:
            listing.append(f"? listing stopped: {error}")
        (target / "tree.txt").write_text("\n".join(listing) + "\n")
        (target / "processes.json").write_text(json.dumps([r.record() | {"stdout": r.stdout[-4000:]} for r in runs], indent=1))

    # -- tests -----------------------------------------------------------
    def test_levels(self, area):
        rng = random.Random(self.rng.random())
        root = area / "basic"
        root.mkdir()
        self.basic_tree(root, rng)
        detail = {}
        for level in LEVELS:
            record, _ = self.roundtrip("basic", root, level)
            detail[level] = record
        source = detail["store"]["source"]["bytes"]
        if detail["store"]["archive_bytes"] < source:
            raise Failure("store archive is smaller than its payload")
        detail["external"] = {"unrar": detail["store"]["unrar"], "sevenzip": detail["store"]["sevenzip"]}
        return detail

    def test_compression_ratio(self, area):
        rng = random.Random(self.rng.random())
        root = area / "text"
        root.mkdir()
        size = 4 * MIB
        self.compressible_tree(root, rng, size)
        detail = {}
        for level in LEVELS:
            record, _ = self.roundtrip("text", root, level, external=(level in ("store", "best")))
            detail[level] = {"archive_bytes": record["archive_bytes"], "seconds": record["create"]["seconds"],
                             "max_rss_kib": record["create"]["max_rss_kib"]}
        store = detail["store"]["archive_bytes"]
        if store < size:
            raise Failure(f"store output {store} smaller than the {size}-byte payload")
        if store > size + 64 * 1024:
            raise Failure(f"store overhead {store - size} bytes is implausibly large")
        for level in LEVELS[1:]:
            ratio = detail[level]["archive_bytes"] / size
            detail[level]["ratio"] = round(ratio, 4)
            if ratio > 0.5:
                raise Failure(f"-l {level} left a text corpus at {ratio:.1%} of its size; compression is not happening")
        if detail["best"]["archive_bytes"] > detail["fast"]["archive_bytes"]:
            raise Failure("best produced a larger archive than fast on compressible text")
        # Same input and options must give identical bytes: the writer is deterministic.
        again = area / "again.rar"
        self.expect_ok(self.arca("create", str(again), str(root), "-l", "normal"), "second normal create")
        first = area / "text-normal.rar"
        detail["deterministic"] = again.read_bytes() == first.read_bytes()
        if not detail["deterministic"]:
            raise Failure("two creates of the same tree at the same level produced different bytes")
        detail["codec_store_equals_level_store"] = True
        via_codec = area / "codec.rar"
        self.expect_ok(self.arca("create", str(via_codec), str(root), "-c", "store"), "create -c store")
        if via_codec.read_bytes() != (area / "text-store.rar").read_bytes():
            raise Failure("-c store and -l store produced different archives")
        return detail

    def test_many_entries(self, area, count):
        rng = random.Random(self.rng.random())
        root = area / "many"
        root.mkdir()
        self.many_tree(root, rng, count)
        record, _ = self.roundtrip("many", root, "fast", timeout=self.opts.timeout * 4)
        record["requested_files"] = count
        if record["source"]["files"] != count:
            raise Failure(f"fixture holds {record['source']['files']} files, expected {count}")
        return record

    def test_large_stream(self, area, total):
        rng = random.Random(self.rng.random())
        root = area / "large"
        self.large_tree(root, rng, total)
        record, archive = self.roundtrip("large", root, "normal", timeout=self.opts.timeout * 8)
        record["requested_bytes"] = total
        return record

    def test_cycles(self, area, count):
        detail = {"cycles": count, "levels": {}, "entries": 0, "bytes": 0, "archive_bytes": 0,
                  "external_checked": 0, "max_rss_kib": 0, "seconds": 0.0}
        for index in range(count):
            seed = self.rng.getrandbits(32)
            rng = random.Random(seed)
            cycle = area / f"c{index:04d}"
            root = cycle / "tree"
            root.mkdir(parents=True)
            self.cycle_tree(root, rng)
            level = rng.choice(LEVELS)
            external = index % 5 == 0 or count <= 20
            try:
                record, _ = self.roundtrip("cycle", root, level, external=external)
            except Failure as failure:
                raise Failure(f"cycle {index} (seed {seed}, level {level}): {failure}")
            detail["levels"][level] = detail["levels"].get(level, 0) + 1
            detail["entries"] += record["entries"]
            detail["bytes"] += record["source"]["bytes"]
            detail["archive_bytes"] += record["archive_bytes"]
            detail["max_rss_kib"] = max(detail["max_rss_kib"], record["create"]["max_rss_kib"])
            detail["seconds"] += record["create"]["seconds"]
            if external and not str(record.get("unrar", "")).startswith("skipped"):
                detail["external_checked"] += 1
            shutil.rmtree(cycle, ignore_errors=True)
        detail["seconds"] = round(detail["seconds"], 3)
        if self.tools.get("unrar") is None:
            detail["unrar"] = "skipped: no official UnRAR"
        if self.tools.get("sevenzip") is None:
            detail["sevenzip"] = "skipped: no 7-Zip with RAR5 support"
        return detail

    def test_refusals(self, area):
        rng = random.Random(self.rng.random())
        src = area / "src"
        src.mkdir()
        self.write(src / "a.txt", self.text_block(rng, 2000))
        self.write(src / "sub" / "b.bin", rng.randbytes(500))
        detail = {}

        def refused(name, args, needle, cwd=None):
            before = sorted(p.name for p in area.iterdir())
            run = self.expect_fail(self.arca(*args, cwd=cwd), name, needle)
            after = sorted(p.name for p in area.iterdir())
            if before != after:
                raise Failure(f"{name}: directory changed from {before} to {after}")
            detail[name] = run.stderr.strip()[-160:]

        existing = area / "existing.rar"
        original = rng.randbytes(1234)
        existing.write_bytes(original)
        refused("existing output", ["create", str(existing), str(src)], "never replaces")
        if existing.read_bytes() != original:
            raise Failure("existing output was modified")
        refused("cbr output", ["create", str(area / "out.cbr"), str(src)], "")
        refused("password", ["create", str(area / "p.rar"), str(src), "-p", "secret"], "encryption")
        refused("hide names", ["create", str(area / "h.rar"), str(src), "-p", "secret", "--hide-names"], "")
        for codec in ("zstd", "deflate", "lzma2"):
            refused(f"codec {codec}", ["create", str(area / f"{codec}.rar"), str(src), "-c", codec], "auto or store")
        refused("threads 4", ["create", str(area / "j.rar"), str(src), "-j", "4"], "sequential")
        refused("output inside source", ["create", str(src / "inside.rar"), str(src)], "output directory")
        refused("source is the output", ["create", str(existing), str(existing)], "")
        dup = area / "dup"
        self.write(dup / "one" / "same.txt", b"1")
        self.write(dup / "two" / "same.txt", b"2")
        refused("duplicate names", ["create", str(area / "dup.rar"), str(dup / "one" / "same.txt"), str(dup / "two" / "same.txt")], "duplicate")
        self.write(dup / "two" / "SAME.TXT", b"3")
        refused("case-colliding names", ["create", str(area / "case.rar"), str(dup / "one" / "same.txt"), str(dup / "two" / "SAME.TXT")], "duplicate")
        if os.name != "nt":
            os.symlink(src / "a.txt", area / "link.txt")
            refused("symlink source", ["create", str(area / "link.rar"), str(area / "link.txt")], "symbolic link")
            os.mkfifo(area / "fifo")
            refused("fifo source", ["create", str(area / "fifo.rar"), str(area / "fifo")], "regular file")
        refused("missing source", ["create", str(area / "missing.rar"), str(area / "absent")], "")
        refused("missing parent", ["create", str(area / "no-such-dir" / "x.rar"), str(src)], "")
        refused("unsupported extension", ["create", str(area / "x.rar5"), str(src)], "")
        self.expect_clean_dir(area, {"src", "existing.rar", "dup", "link.txt", "fifo"})
        return detail

    def test_limits(self, area):
        """Sparse files make the 4 GiB member and 16 GiB aggregate ceilings cheap to hit."""
        detail = {}
        big = area / "big"
        big.mkdir()
        try:
            with open(big / "member.bin", "wb") as f:
                f.truncate(4 * 1024 * MIB + 1)
            allocated = os.stat(big / "member.bin").st_blocks * 512
            if allocated > 16 * MIB:
                raise Skip("filesystem does not support sparse files")
        except OSError as error:
            raise Skip(f"cannot create sparse fixtures: {error}")
        run = self.expect_fail(self.arca("create", str(area / "member.rar"), str(big)), "4 GiB member", "exceeds")
        detail["member_over_4gib"] = run.stderr.strip()[-160:]
        (big / "member.bin").unlink()
        aggregate = area / "aggregate"
        aggregate.mkdir()
        for index in range(5):
            with open(aggregate / f"part{index}.bin", "wb") as f:
                f.truncate(4 * 1024 * MIB - 1)
        run = self.expect_fail(self.arca("create", str(area / "aggregate.rar"), str(aggregate)), "16 GiB aggregate", "exceeds")
        detail["aggregate_over_16gib"] = run.stderr.strip()[-160:]
        deep = area / "deep"
        deep.mkdir()
        component = "n" * 200
        cwd = os.getcwd()
        try:
            os.chdir(deep)
            for _ in range(22):
                os.mkdir(component)
                os.chdir(component)
            Path("leaf.txt").write_bytes(b"leaf")
        finally:
            os.chdir(cwd)
        run = self.expect_fail(self.arca("create", "deep.rar", "deep", cwd=area), "4096-byte name")
        # A 4400-byte member name also exceeds PATH_MAX, so the OS may refuse
        # the path before the writer's own ceiling; the ceiling is unit-tested
        # with injected bounds in arca-cli. Either way nothing may be written.
        detail["name_over_4096_bytes"] = {"rc": run.rc, "message": run.stderr.strip()[-160:],
                                          "ceiling_reported": "exceeds" in run.stderr}
        self.expect_clean_dir(area, {"big", "aggregate", "deep"})
        return detail

    def test_entry_ceiling(self, area):
        root = area / "entries"
        for index in range(100):
            sub = root / f"d{index:03d}"
            sub.mkdir(parents=True)
            for inner in range(1000):
                (sub / f"{inner:04d}").touch()
        run = self.expect_fail(self.arca("create", str(area / "entries.rar"), str(root), timeout=self.opts.timeout * 4),
                               "100000 entries", "more than 100000 members")
        self.expect_clean_dir(area, {"entries"})
        return {"entries_over_100000": run.stderr.strip()[-160:], "files": 100000, "dirs": 101}

    def test_corruption(self, area):
        rng = random.Random(self.rng.random())
        root = area / "tree"
        root.mkdir()
        self.basic_tree(root, rng)
        record, archive = self.roundtrip("victim", root, "normal", external=False)
        data = bytearray(archive.read_bytes())
        detail = {"archive_bytes": len(data), "flips": 0, "truncations": 0, "unrar_agreed": 0, "sevenzip_agreed": 0}
        offsets = sorted(set([8, 20, 40, len(data) // 2, len(data) - 1] + [rng.randrange(8, len(data)) for _ in range(11)]))
        for index, offset in enumerate(offsets):
            mutated = bytearray(data)
            mutated[offset] ^= 1 << rng.randrange(8)
            self.corrupt_case(area, f"flip{index:02d}", mutated, detail)
            detail["flips"] += 1
        for index, cut in enumerate(sorted({8, 20, len(data) // 3, len(data) - 1, len(data) - 7, rng.randrange(9, len(data))})):
            self.corrupt_case(area, f"trunc{index:02d}", data[:cut], detail)
            detail["truncations"] += 1
        self.corrupt_case(area, "empty", b"", detail)
        self.corrupt_case(area, "garbage", rng.randbytes(4096), detail)
        self.corrupt_case(area, "zip-magic", b"PK\x03\x04" + rng.randbytes(1000), detail)
        return detail

    def corrupt_case(self, area, name, payload, detail):
        bad = area / f"{name}.rar"
        bad.write_bytes(payload)
        dest = area / f"{name}-out"
        dest.mkdir()
        self.expect_fail(self.arca("test", str(bad)), f"arca test {name}")
        self.expect_fail(self.arca("extract", str(bad), "-o", str(dest)), f"arca extract {name}")
        leftovers = sorted(p.relative_to(dest).as_posix() for p in dest.rglob("*"))
        if leftovers:
            raise Failure(f"{name}: extraction of a corrupt archive published {leftovers[:5]}")
        tool = self.tools.get("unrar")
        if tool and self.run([tool["path"], "t", "-inul", str(bad)]).rc != 0:
            detail["unrar_agreed"] += 1
        tool = self.tools.get("sevenzip")
        if tool and self.run([tool["path"], "t", "-bso0", "-bsp0", str(bad)]).rc != 0:
            detail["sevenzip_agreed"] += 1
        bad.unlink()
        dest.rmdir()

    def test_interrupt(self, area, total):
        """SIGINT and SIGTERM mid-write must be honoured cooperatively: the CLI
        exits by itself with a nonzero status and 'cancelled' on stderr, nothing
        ever appears at the output path and no staged `.arca-*.rar.part` file is
        left in the directory. A run that finishes before the signal fails in
        required-tool mode, otherwise skips. SIGKILL and power loss are not covered."""
        rng = random.Random(self.rng.random())
        root = area / "large"
        self.large_tree(root, rng, max(total, 32 * MIB))
        detail = {}
        for sig in (signal.SIGINT, signal.SIGTERM):
            out = area / f"{sig.name}.rar"
            run = self.arca("create", str(out), str(root), "-l", "best", signal_after=0.5, sig=sig)
            temps = sorted(p.name for p in area.iterdir() if TEMP_PATTERN.match(p.name))
            if run.rc == 0:
                reason = (f"{sig.name}: creation finished in {run.seconds:.2f}s before the signal; "
                          "raise --large-mib to exercise interruption")
                raise Failure(reason) if self.opts.require_tools else Skip(reason)
            if out.exists():
                raise Failure(f"{sig.name}: an output file exists after interruption")
            if temps:
                raise Failure(f"{sig.name}: staged files left behind: {temps}")
            if run.rc < 0:
                raise Failure(f"{sig.name}: the CLI died from the signal (rc={run.rc}) instead of cancelling")
            if "cancelled" not in run.stderr.lower():
                raise Failure(f"{sig.name}: rc={run.rc} without a cancellation message: {run.stderr.strip()[-200:]}")
            detail[sig.name] = {"rc": run.rc, "seconds": round(run.seconds, 3),
                                "stderr": run.stderr.strip()[-120:], "output_exists": False, "staged_left_behind": []}
        return detail

    def test_disk_full(self, area):
        if platform.system() != "Linux" or not shutil.which("unshare"):
            raise Skip("needs Linux unshare for a private tmpfs")
        probe = subprocess.run(["unshare", "--user", "--map-root-user", "--mount", "true"], capture_output=True)
        if probe.returncode != 0:
            raise Skip(f"unshare user/mount namespaces unavailable: {probe.stderr.decode(errors='replace').strip()[-200:]}")
        rng = random.Random(self.rng.random())
        root = area / "src"
        root.mkdir()
        self.write(root / "random.bin", rng.randbytes(3 * MIB))
        self.write(root / "keep.txt", b"keep\n")
        full = area / "full"
        full.mkdir()
        script = (
            "mount -t tmpfs -o size=1m,mode=700 tmpfs \"$1\" && "
            "printf original > \"$1/existing.rar\" && "
            "\"$2\" create \"$1/out.rar\" \"$3\" -l store; rc=$?; "
            "ls -A \"$1\" > \"$4\"; cat \"$1/existing.rar\" >> \"$4\"; exit $rc"
        )
        report = area / "report.txt"
        run = self.run(["unshare", "--user", "--map-root-user", "--mount", "sh", "-c", script, "sh",
                        str(full), str(self.opts.arca), str(root), str(report)])
        if run.rc == 0:
            raise Failure("creating a 3 MiB archive inside a 1 MiB tmpfs succeeded")
        listing = report.read_text().splitlines() if report.exists() else []
        if listing[:-1] != ["existing.rar"] or (listing and listing[-1] != "original"):
            raise Failure(f"disk-full tmpfs holds unexpected entries or a modified file: {listing}")
        return {"rc": run.rc, "stderr": run.stderr.strip()[-200:], "tmpfs_after": listing[:-1]}

    # -- main ------------------------------------------------------------
    def main(self):
        opts = self.opts
        print(f"arca RAR creation harness: seed={opts.seed} mode={'stress' if opts.stress else 'smoke'} "
              f"work={self.work} output={self.out}", flush=True)
        self.discover_tools()
        for name, tool in self.tools.items():
            print(f"  tool {name}: {tool['version'] if tool else 'not available'}", flush=True)
        if opts.require_tools and (not self.tools["unrar"] or not self.tools["sevenzip"]):
            print("official UnRAR and RAR5-capable 7-Zip are required (--require-tools)", file=sys.stderr)
            self.write_report(1)
            return 1
        self.test("levels", self.test_levels)
        self.test("compression-ratio", self.test_compression_ratio)
        self.test("refusals", self.test_refusals)
        self.test("limits", self.test_limits)
        self.test("corruption", self.test_corruption)
        self.test("disk-full", self.test_disk_full)
        self.test("many-entries", self.test_many_entries, count=opts.many)
        self.test("large-stream", self.test_large_stream, total=opts.large_mib * MIB)
        self.test("interrupt", self.test_interrupt, total=opts.large_mib * MIB)
        self.test("cycles", self.test_cycles, count=opts.runs)
        if opts.stress:
            self.test("entry-ceiling", self.test_entry_ceiling)
        failed = [r for r in self.results if r["status"] == "fail"]
        self.write_report(1 if failed else 0)
        if not opts.keep:
            shutil.rmtree(self.work, ignore_errors=True)
        counts = {s: sum(1 for r in self.results if r["status"] == s) for s in ("pass", "warn", "fail", "skip")}
        print(f"summary: {counts}; report {self.out / 'report.json'}", flush=True)
        return 1 if failed else 0

    def write_report(self, exit_code):
        report = {
            "harness": HARNESS_VERSION,
            "started": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "seed": self.opts.seed,
            "mode": "stress" if self.opts.stress else "smoke",
            "options": {"runs": self.opts.runs, "many": self.opts.many, "large_mib": self.opts.large_mib,
                        "timeout_s": self.opts.timeout, "memory_mib": self.opts.memory_mib,
                        "require_tools": self.opts.require_tools},
            "host": {"platform": platform.platform(), "python": platform.python_version(), "cpus": os.cpu_count()},
            "tools": self.tools,
            "results": self.results,
            "processes": len(self.runs),
            "exit_code": exit_code,
        }
        (self.out / "report.json").write_text(json.dumps(report, indent=1, ensure_ascii=False) + "\n")


def parse(argv):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--arca", default=os.environ.get("ARCA", str(ROOT / "target" / "release" / "arca")),
                        help="arca binary (default: target/release/arca or $ARCA)")
    parser.add_argument("--output", default=str(ROOT / "target" / "rar-create-stress"), help="report and failure artifacts")
    parser.add_argument("--workdir", default=None, help="parent for scratch trees (default: system temp)")
    parser.add_argument("--seed", type=int, default=20261004)
    parser.add_argument("--stress", action="store_true", help="large dataset, 10k entries and 100 cycles")
    parser.add_argument("--runs", type=int, default=None, help="create/list/test/extract cycles (smoke 8, stress 100)")
    parser.add_argument("--many", type=int, default=None, help="files in the many-entries archive (smoke 400, stress 10000)")
    parser.add_argument("--large-mib", type=int, default=None, help="size of the large stream (smoke 8, stress 128)")
    parser.add_argument("--timeout", type=float, default=120.0, help="seconds per child process (x4 many, x8 large)")
    parser.add_argument("--memory-mib", type=int, default=4096, help="RLIMIT_AS for every child process")
    parser.add_argument("--unrar", default=None, help="official UnRAR binary (or $UNRAR / PATH)")
    parser.add_argument("--sevenzip", default=None, help="7-Zip binary with RAR5 support (or $SEVENZIP / 7zz / 7z)")
    parser.add_argument("--require-tools", action="store_true", help="fail instead of skipping when UnRAR or 7-Zip is missing")
    parser.add_argument("--keep", action="store_true", help="keep scratch trees and artifacts even on success")
    opts = parser.parse_args(argv)
    opts.runs = opts.runs if opts.runs is not None else (100 if opts.stress else 8)
    opts.many = opts.many if opts.many is not None else (10000 if opts.stress else 400)
    opts.large_mib = opts.large_mib if opts.large_mib is not None else (128 if opts.stress else 8)
    opts.arca = str(Path(opts.arca).resolve())
    if not Path(opts.arca).is_file():
        parser.error(f"arca binary not found at {opts.arca}; build it with cargo build --release -p arca-cli")
    if not FIXTURE_RAR5.is_file():
        parser.error(f"missing fixture {FIXTURE_RAR5}")
    return opts


if __name__ == "__main__":
    sys.exit(Harness(parse(sys.argv[1:])).main())
