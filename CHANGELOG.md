# Changelog

All notable changes to iroh-gossip will be documented in this file.

## [0.30.1](https://github.com/n0-computer/iroh-gossip/compare/v0.30.0..0.30.1) - 2024-12-20

### 🐛 Bug Fixes

- Add missing Sync bound to EventStream's inner - ([d7039c4](https://github.com/n0-computer/iroh-gossip/commit/d7039c4684e0072bce1c1fe4bce7d39ba42e8390))

## [0.30.0](https://github.com/n0-computer/iroh-gossip/compare/v0.29.0..0.30.0) - 2024-12-17

### ⛰️  Features

- Remove rpc from default features - ([10e9b68](https://github.com/n0-computer/iroh-gossip/commit/10e9b685f6ede483ace4be4360466a111dfcfec4))
- [**breaking**] Introduce builder pattern to construct Gossip ([#17](https://github.com/n0-computer/iroh-gossip/issues/17)) - ([0e6fd20](https://github.com/n0-computer/iroh-gossip/commit/0e6fd20203c6468af9d783f1e62379eca283188a))
- Update to iroh 0.30 - ([b3a5a33](https://github.com/n0-computer/iroh-gossip/commit/b3a5a33351b57e01cba816826d642f3314f00e7d))

### 🐛 Bug Fixes

- Improve connection handling ([#22](https://github.com/n0-computer/iroh-gossip/issues/22)) - ([61e64c7](https://github.com/n0-computer/iroh-gossip/commit/61e64c79961640cd2aa2412e607035cd7750f824))
- Prevent task leak for rpc handler task ([#20](https://github.com/n0-computer/iroh-gossip/issues/20)) - ([03db85d](https://github.com/n0-computer/iroh-gossip/commit/03db85d218738df7b4c39cc2d178f2f90ba58ea3))

### 🚜 Refactor

- Adapt ProtocolHandler impl ([#16](https://github.com/n0-computer/iroh-gossip/issues/16)) - ([d5285e7](https://github.com/n0-computer/iroh-gossip/commit/d5285e7240da4e233be7c8f83099741f6f272bb0))
- [**breaking**] Align api naming between RPC and direct calls  - ([35d73db](https://github.com/n0-computer/iroh-gossip/commit/35d73db8a982d7bbe1eb3cba126ac25422f5c1b6))
- Manually track dials, instead of using `iroh::dialer` ([#21](https://github.com/n0-computer/iroh-gossip/issues/21)) - ([2d90828](https://github.com/n0-computer/iroh-gossip/commit/2d90828a682574e382f5b0fbc43395ff698a63e2))

### 📚 Documentation

- Add "Getting Started" to the README and add the readme to the docs ([#19](https://github.com/n0-computer/iroh-gossip/issues/19)) - ([1625123](https://github.com/n0-computer/iroh-gossip/commit/1625123a89278cb09827abe8e7ee2bf409cf2f20))

## [0.29.0](https://github.com/n0-computer/iroh-gossip/compare/v0.28.1..0.29.0) - 2024-12-04

### ⛰️  Features

- Add cli - ([16f3505](https://github.com/n0-computer/iroh-gossip/commit/16f35050fe47534052e79dcbca42da4212dc6256))
- Update to latest iroh ([#11](https://github.com/n0-computer/iroh-gossip/issues/11)) - ([89e91a3](https://github.com/n0-computer/iroh-gossip/commit/89e91a34bd046fb7fbd504b2b8d0849e2865d410))
- Reexport ALPN at top level - ([7a0ec63](https://github.com/n0-computer/iroh-gossip/commit/7a0ec63a0ab7f14d78c77f8c779b2abef956da40))
- Update to iroh@0.29.0  - ([a28327c](https://github.com/n0-computer/iroh-gossip/commit/a28327ca512407a18a3802800c6712adc33acf84))

### 🚜 Refactor

- Use hex for debugging and display - ([b487112](https://github.com/n0-computer/iroh-gossip/commit/b4871121ed1862da4459353f63415d8ae4b3f8c5))

### ⚙️ Miscellaneous Tasks

- Fixup deny.toml - ([e614d86](https://github.com/n0-computer/iroh-gossip/commit/e614d86c0a690ac4acb6b4ef394a0bf55662dcc7))
- Prune some deps ([#8](https://github.com/n0-computer/iroh-gossip/issues/8)) - ([ba0f6b0](https://github.com/n0-computer/iroh-gossip/commit/ba0f6b0f54a740d8eae7ee6683f4aa1d8d8c8eb2))
- Init changelog - ([3eb675b](https://github.com/n0-computer/iroh-gossip/commit/3eb675b6a1ad51279ce225d0b36ef9957f17aa06))
- Fix changelog generation - ([95a4611](https://github.com/n0-computer/iroh-gossip/commit/95a4611aafee248052d3dc9ef97c9bc8a26d4821))

## [0.28.1](https://github.com/n0-computer/iroh-gossip/compare/v0.28.0..v0.28.1) - 2024-11-04

### 🐛 Bug Fixes

- Update to quic-rpc@0.14 - ([7b73408](https://github.com/n0-computer/iroh-gossip/commit/7b73408e80381b77534ae3721be0421da110de80))
- Use correctly patched iroh-net - ([276e36a](https://github.com/n0-computer/iroh-gossip/commit/276e36aa1caff8d41f89d57d8aef229ffa9924cb))

### ⚙️ Miscellaneous Tasks

- Release iroh-gossip version 0.28.1 - ([efce3e1](https://github.com/n0-computer/iroh-gossip/commit/efce3e1dc991c15a7f1fc6f579f04876a22a7b1e))


