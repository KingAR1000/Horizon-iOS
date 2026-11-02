# iOS integration dependencies

Horizon is MIT licensed. Its copyright and third-party notices are retained in `engine/LICENSE` and `engine/THIRD_PARTY_NOTICES.md` and included in the app.

The on-device preparer embeds [Ristretto 0.34.0](https://github.com/theseus-rs/ristretto), copyright Brian Heineman and contributors, under MIT/Apache-2.0. `patches/ristretto-gc.patch` replaces recursive graph marking with queued marking. `patches/ristretto.patch` corrects speculative method resolution, implements scoped-value thread caches/binding access, adapts Darwin filesystem operations for iOS, and returns embedded VM exits to the host instead of terminating the app. Original license texts from commit `8448588ecfcedf79a59d697939493f289c1c6ad7` are retained in `ios/Licenses` and packaged with the app.

[sysinfo 0.39.6](https://github.com/GuillaumeGomez/sysinfo), copyright Guillaume Gomez, is MIT licensed. `patches/sysinfo.patch` supplies a Darwin routing ABI constant missing from libc's iOS exports. The dependency's license is preserved and packaged.

The pinned Amazon Corretto Java class libraries are downloaded separately during setup. Their original legal notices are retained in the installed runtime's `legal` directory. The application does not execute downloaded native JVM binaries. Mojang's game files are downloaded separately and remain subject to Mojang's terms; they are excluded from source and app bundles.
