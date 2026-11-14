# Contributing to VELOCITY

Thanks for helping improve VELOCITY.

## Principles

- **Local-first & privacy-respecting:** no mandatory accounts/cloud services. Never commit personal training data, exports, secrets, tokens, or device identifiers.
- **Free/open only:** use dependencies, datasets, and assets with licenses compatible with open-source distribution (prefer MIT/Apache-2.0/BSD where possible). Document attribution and license terms for any added data/assets.
- **Performance-aware:** profile and measure (FPS/frame time, CPU/GPU/RAM, load times). Prefer LOD, culling, instancing, streaming, and adaptive quality over brute-force rendering.
- **No root/system changes:** do not use `sudo`/root privileges or require system-wide installs in project workflows.

## Development

1. Build and run the native app:
   ```bash
   cargo run -p velocity-app
   ```
2. Run checks/tests before submitting changes:
   ```bash
   cargo check -p velocity-app
   cargo test -p velocity-core -p velocity-app
   ```
3. For rendering/world changes, verify visuals in-app (and capture screenshots when useful) and note performance impact.

## Pull requests

- Keep changes focused and describe user-visible behavior, technical approach, and verification performed.
- Call out any new datasets/assets and include their source + license/attribution.
- Do not include secrets or personal data in commits, logs, or screenshots.

## Reporting issues

Include platform/GPU info, steps to reproduce, expected vs actual behavior, logs/errors, and (for graphics issues) screenshots and the selected quality/settings.
