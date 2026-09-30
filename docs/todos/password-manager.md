# Password manager

Status: removed from the code, pending a full rewrite. Tracked in [#6](https://github.com/beyondhumane/arca/issues/6).

The default password was removed because it was a quarter of a feature, and the quarter that pays off least.

- `04b4fc1`, `feat(arca-gui): offer the default password wherever one is asked for`, is the version that was removed.

## Why this document exists

Arca had a "default password". It was set with `Ctrl+P`, lived in memory until the window closed, and appeared as a *Use the default password* button in the four places where one is asked for: opening, extracting, changing an archive's password and choosing one for a new archive.

The button unlocked nothing. It only wrote the text into the field:

```rust
this.controller.state.password_input = kept.clone();
```

That was deliberate, and the reasoning was sound for **setting** a password: setting one by mistake costs rewriting the whole archive to remove it. But it applied just the same to **opening**, where there is nothing to rewrite and a failed attempt costs nothing.

The result was a feature that saved typing a long password at the cost of two clicks, only within one session, and that had to be enabled first with `Ctrl+P`. If you open one encrypted archive a day, it costs more than it saves. It only starts to pay off with a whole folder of archives sharing one password, and that is exactly the case it covered worst, because it still asked for two clicks per archive.

On top of that, its doc comment promised something the code did not do ("One password **to try before asking**"; it was never tried on its own), and it pointed to a function, `default_password_window`, that did not exist in the tree.

## What the competition does

Checked, not assumed.

| | Has it? | How |
| --- | --- | --- |
| 7-Zip | No | No configurable default password. It does cache passwords in memory on its own during the session, and that draws the opposite complaint: its forum has a thread titled *"How to stop caching passwords"* |
| NanaZip | No | Inherits 7-Zip's behaviour. Open request [M2Team/NanaZip#321](https://github.com/M2Team/NanaZip/issues/321), *"Add password manager with auto-try fill"*. A whole fork, `SanRive/NanaZipPasswordManager`, exists just to add it |
| WinRAR | Yes | *Organize passwords* dialog, with labelled, persistent entries |

The demand is real: the competition gets asked for it, and in NanaZip's case someone forked the project to get it. The conclusion is not that the idea was bad, but that half an implementation does not work.

## What rebuilding it would take

WinRAR has already solved the design, and it is worth copying its four pieces, because each one answers a question the removed version left open.

1. **Archive mask per entry** (`Select for archives`). A password is tied to a pattern, not to "the session". This is what makes the folder of archives sharing one password work, which is the only case that justifies the feature.
2. **"Use without confirmation" checkbox** per entry. This is auto-try, but as an option and per password, not as fixed behaviour. The 7-Zip warning applies right here: caching without asking annoys people, so the caution of the removed version (offer, do not apply) should stay the default.
3. **Master password.** It is the price of persisting across sessions. Without it nothing is saved: a plaintext password in `gui.conf`, next to the theme and column widths, is how an encrypted archive stops being encrypted.
4. **Labels**, so the list can be read and each entry identified without showing the password text.

## The security part, which is not optional

Today no password touches the disk. `Settings`, the only thing written, has no field to put one in; every password lives in `AppState`, in memory, and dies with the process. Any future version has to keep that property unless it ships with the master password from point 3.

Separately, and already true of the current tree: there is no `zeroize` or `secrecy` in any `Cargo.toml`. Passwords are plain `String`s, so they are not overwritten when freed and can end up in `pagefile.sys` or in a crash dump. That protects against "someone reads my config file", which was the goal, but not against forensic memory analysis. A manager that persists raises the stakes considerably, so `Zeroizing<String>` should come in the same change, taking care with the clones handed to worker threads.

## What exactly was removed

So it can be rebuilt without digging through history:

- `AppState::default_password` and `AppState::asking_default_password`.
- `ModalKind::DefaultPassword` and its dialog.
- `OverflowAction::DefaultPassword` and its menu entry.
- `Shortcut::DefaultPassword` and the `Ctrl+P` shortcut.
- `keep_default_password` and `forget_default_password`.
- Both *Use the default password* buttons: the one in the password prompt and the one in the compress box.
- The strings `default_password`, `use_default_password`, `password_kept` and `password_forgotten`. `password_hint` stays, because the regular dialog also uses it.
