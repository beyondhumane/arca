---
description: How to build, test and contribute, and the Apache-2.0 license.
group: Reference
order: 16
keywords: contribute pull request issue bug license apache
---

# Contributing & license

Arca is developed in the open on GitHub. Bug reports, benchmarks from your own hardware and pull requests are all welcome.

## Set up

```sh
git clone https://github.com/beyondhumane/arca
cd arca
cargo build --release
cargo test --workspace
bash interop.sh            # needs zip, unzip, tar and 7-Zip
```

## Ground rules

- **Parsers stay safe.** Nothing that reads archive bytes gets unsafe code, and the crate-level forbid makes it a compile error anyway.
- **Tests come with fixes.** Every bug fixed is a regression test added.
- **interop.sh stays green.** Whatever Arca writes must open elsewhere, and vice versa.
- **Benchmarks come with their command.** Best of several runs, alternate the arms, publish the machine and the losses.

## Reporting a bug

[Open an issue](https://github.com/beyondhumane/arca/issues/new) and include `arca --version`, your operating system, the exact command you ran and what happened. If you can share the archive, or a smaller one that reproduces the problem, even better. Browse [existing issues](https://github.com/beyondhumane/arca/issues) first.

## License

Arca is licensed under the [Apache License 2.0](https://github.com/beyondhumane/arca/blob/main/LICENSE). You may use, modify and distribute it, commercially or not, provided you keep the license and notices. The license includes an express grant of patent rights from contributors.
