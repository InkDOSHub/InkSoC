# InkSoC roadmap

InkSoC runs Windows programs in the browser with an x86-64 engine (JIT to WebAssembly) and Wine.

Written on 2026-11-20 during the InkOS autopilot run. Ordered by priority; nothing here is started unless marked.

## Now

- Adopt the InkOS postMessage protocol v1 (`docs/PROTOCOL.md`): theme via `ink-tema`, files via `inkos-vfs-file` (`inkos:file` event from the Hub's `assets/tema.js`).
- Validate parent origin against the InkOS allowlist before acting on any message.
- License review: `src/wine/` holds LGPL-2.1 Wine sources inside the MIT tree; move to a separate directory or artifact (flagged in THIRD_PARTY_LICENSES.md).
- Expose the engine's guest RAM to the InkOS pager (software paging, ADR-swap in InkOS).

## Next

- Test on iPhone 17 Pro Max and iPad 11" (A16), iOS 27 Safari, and record results.
- Keep third-party components listed in `THIRD_PARTY_LICENSES.md` up to date.

## Not planned in this round

- Large new features. This file only records direction; see the README for the current state.
