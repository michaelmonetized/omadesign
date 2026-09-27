# Rust dependency licenses and notices

`licenses.json` contains the Linux dependency inventory, full selected license
texts, copyright notices, and additional packaged NOTICE/COPYRIGHT files for the
two supported Linux architectures. It includes build dependencies to retain
notices for statically bundled native code. The Credits tab reads this same file.
No development machine paths are included.

Regenerate after changing Cargo.lock:

```
cargo install cargo-about --version 0.9.2 --locked --features cli
python3 scripts/update-third-party.py
```

The generator uses `about.toml` and refuses unresolved license expressions.
Native libraries, models, fonts/icons and their additional notices are also
listed in Credits and shipped in the adjacent license directories.
