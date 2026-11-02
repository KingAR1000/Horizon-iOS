# iOS integration dependencies

Horizon is MIT licensed. Its copyright and third-party notices are retained in `engine/LICENSE` and `engine/THIRD_PARTY_NOTICES.md` and included in the app.

The on-device preparer embeds [Ristretto 0.34.0](https://github.com/theseus-rs/ristretto), copyright Brian Heineman and contributors, under MIT/Apache-2.0. `patches/ristretto.patch` corrects speculative method resolution, implements scoped-value thread caches/binding access, and adapts Darwin filesystem operations for iOS. Original license texts are preserved in the downloaded dependency and packaged with the app.

[sysinfo 0.39.6](https://github.com/GuillaumeGomez/sysinfo), copyright Guillaume Gomez, is MIT licensed. `patches/sysinfo.patch` supplies a Darwin routing ABI constant missing from libc's iOS exports. The dependency's license is preserved and packaged.

The pinned Amazon Corretto Java class libraries are downloaded separately during setup. Their original legal notices are retained in the installed runtime's `legal` directory. The application does not execute downloaded native JVM binaries. Mojang's game files are downloaded separately and remain subject to Mojang's terms; they are excluded from source and app bundles.
