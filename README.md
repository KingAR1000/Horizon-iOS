# Horizon for iPhone

An iOS integration of [Horizon](https://github.com/horizon-rust/horizon), using its real Rust gameplay and wgpu Metal renderer. iOS 16+, arm64 iPhone. The engine is pinned as a submodule; `patches/ios.patch` contains the platform changes. MIT; upstream attribution and bundled dependency licenses are retained.

## Install

Run **Actions → Build unsigned iOS IPA → Run workflow**, then download the `Horizon-unsigned` artifact. Sign `Horizon-unsigned.ipa` with SideStore, AltStore, or your preferred signing tool and install it. No Apple account or certificate is used by the build.

Prepare resources from your own Minecraft installation using the pinned engine's [build instructions](engine/README.md#build-from-source) on a desktop. Copy the resulting folder containing `horizon-resources.json` to Files on the phone. Builds configured with `HORIZON_MSA_CLIENT_ID` offer **Sign in with Microsoft**, check Java Edition ownership and download checksum-verified official JARs, libraries and assets into Documents/minecraft. Account tokens stay in iOS Keychain; valid game sessions are loaded on the next launch.

These downloads do **not** include Horizon’s generated gameplay tables: Mojang does not distribute them, and upstream generates them using Java 25. Desktop preparation is still required. Select **Import resources**, choose that folder, then close and reopen Horizon. Game files, saves, settings and logs live in Horizon's Documents folder, accessible through Files/Finder. Proprietary game resources are never distributed here.

Move with the left touch pad, drag the right half to look, and use the labeled action buttons. Tap menus directly; **Keyboard** opens text input. The arrow buttons cycle hotbar slots. In menus, **Use** right-clicks the last touched position (for splitting inventory stacks). Graphics settings are adjustable in game; start with the conservative defaults on older phones.

## Build

On macOS with Xcode 16.4 and Rust installed:

```sh
git clone --recurse-submodules https://github.com/KingAR1000/Horizon-iOS.git
cd Horizon-iOS
./scripts/build.sh
```

The script applies the pinned patch, builds with the committed lockfile and Rust toolchain, and packages `build/Horizon-unsigned.ipa`. The workflow is manual only.

## JIT and memory

Horizon compiles Rust gameplay ahead of time to native ARM64. It has no JVM or gameplay bytecode interpreter: JIT activation provides no benefit and is unnecessary, including when Apple's code-signing restrictions prohibit JIT. Metal shaders compile through Apple's system APIs. Desktop native code mods cannot be compiled or loaded on-device; statically linked Rust mods require a rebuild.

`ios/Memory.entitlements` supplies optional increased-memory and virtual-address-space requests for signing profiles that support them. Ordinary free-account signing may strip or reject these entitlements; the standard IPA does not require them. Neither these requests nor JIT tools disable iOS's memory limits. Use short render distances; iOS can terminate memory-heavy worlds.

## Limitations

This port is under validation; a successful package build does not establish on-device gameplay stability. No physical iPhone testing has been performed. Narrator and gamepad integration are not ported. Offline-mode multiplayer is available through Horizon; authenticated servers require an upstream-compatible session. Resource preparation requires a desktop. Suspension saves the integrated server and pauses play, but iOS can terminate the app without allowing a final save. Upstream feature and compatibility limitations also apply.

Microsoft sign-in requires the repository Actions variable `HORIZON_MSA_CLIENT_ID`: an Azure public-client registration approved for Minecraft API access. Obtain an ID authorized by the Horizon team or register your own using [upstream’s instructions](engine/docs/microsoft-account.md). No client secret is needed. An unconfigured build reports that sign-in is unavailable. End-to-end account authentication has not been tested with an approved ID.
