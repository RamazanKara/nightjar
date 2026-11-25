# Nightjar

Host-neutral Moonlight-protocol client for Android and desktop, mirrored to RamazanKara/nightjar.

- Start from the [code map](docs/code-map.md) and its narrowest test.
- Gate host extras by `HostCapabilities`, never by brand; user-facing text says Nightjar.
- Android and desktop stay at parity, shared logic in the `client/protocol` crate; touch controls are Android-only.
- Desktop builds for Windows, Linux and macOS: gate Linux-only code with `target_os = "linux"` (`cfg(unix)` includes macOS); a changed shader `.spv` needs `protocol/desktop-media/src/metal/generate.py`.
- Fewer settings: a safe, measured default instead of a new setting; merge related ones, drop always-right switches, hide what doesn't apply. A removed, merged or renamed key migrates or clears its stored values, and a retired name is never reused (`SettingsKeysTest`). A setting's help says what the player gets, what it costs and when to change it, in two short sentences, in both apps and all translations.
- Never change applicationId `com.butterpollo.client` or the signing key.

## Release signing

`keystore.properties` lives outside the repository: copy it to `client/` for a release build and delete it after; never print or commit it. `apksigner verify --print-certs` must show SHA-256 `312ed322c7c29d1aec4fae4a3979ebcb557af44d821d7f4746c0968548eb630b`.
