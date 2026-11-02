# Horizon for iPhone

An iOS port of [Horizon](https://github.com/horizon-rust/horizon), with its real Rust gameplay and Metal renderer. iOS 16+, arm64 iPhone. MIT; upstream and dependency attribution is retained.

## Install and set up

Run **Actions → Build unsigned iOS IPA → Run workflow** and download `Horizon-unsigned`. Sign the IPA using SideStore, AltStore, or another signing tool, then install it.

Tap **Sign in with Microsoft** using an account that owns Minecraft: Java Edition. Horizon downloads checksum-verified game JARs, libraries, textures, sounds and languages from Mojang. Its embedded Rust Java interpreter runs upstream's resource exporters on the phone. Keep Horizon open during preparation, then close and reopen it to play. No resource files from a PC are required by this setup path. Verified downloads and completed preparation steps are reused after interruption. Tokens are stored in iOS Keychain.

**On-device preparation is experimental and is still being validated against the complete exporters.** It is slow and memory intensive; lower-memory phones may be terminated by iOS. A successful IPA build alone does not verify setup or gameplay on a physical iPhone. Errors are displayed in the app; `preparation.log`, downloads, resources and saves are available in Horizon's Documents folder through Files. Optional folder import remains available for recovery. No Mojang files are committed or included in the IPA.

Move with the left pad, drag the right half to look, and use the action buttons. Touch menus directly; **Keyboard** opens text input. Arrows cycle hotbar slots; **Use** right-clicks the last touched menu position.

## Build

On macOS with Xcode 16.4, Rust, Python 3 and JDK 25.0.4 installed:

```sh
git clone --recurse-submodules https://github.com/KingAR1000/Horizon-iOS.git
cd Horizon-iOS
./scripts/build.sh
```

The script applies pinned compatibility patches, compiles Horizon-owned exporter classes against verified build inputs, builds native ARM64 with the committed Cargo lockfile, and packages `build/Horizon-unsigned.ipa`. Game inputs used during compilation are excluded from the app. The Actions workflow is manual only.

## JIT, memory and limitations

Rust gameplay is compiled ahead of time. JIT activation is unnecessary for gameplay; resource preparation uses an interpreter so it also works under code-signing restrictions that forbid JIT. Metal shader compilation uses Apple's APIs. Desktop native code mods require a rebuild.

`ios/Memory.entitlements` provides optional increased-memory and address-space requests for signing profiles that support them. Free-account signing may strip or reject them; they do not remove iOS memory limits. Use short render distances. No physical iPhone testing has been performed. Narrator and gamepad integration remain unported. Suspension requests a save, but iOS may terminate the app before it finishes. Upstream feature and compatibility limits apply.

The workflow defaults to Horizon's public Microsoft client ID, `331e08f8-172d-43b9-835a-3108c2fcdbf3`, from its [launcher build logs](https://github.com/horizon-rust/horizon/actions/runs/41502948299/job/125477050120). Override it using the repository variable `HORIZON_MSA_CLIENT_ID` for your own Mojang-approved registration. No client secret is used. Account sign-in has not been verified end to end.
