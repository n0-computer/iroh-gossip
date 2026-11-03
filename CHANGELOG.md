# Changelog

All notable changes to iroh-gossip will be documented in this file.

## [0.102.0](https://github.com/n0-computer/iroh-gossip/compare/v0.101.0..0.102.0) - 2026-11-03

### ⛰️  Features

- *(net)* A proper send queue where a slow peer doesn't block the actor anymore ([#185](https://github.com/n0-computer/iroh-gossip/issues/185)) - ([dee7a26](https://github.com/n0-computer/iroh-gossip/commit/dee7a261d1772d19a213a6889620461f17630ad3))
- *(proto)* Also rejoin through a recent neighbor ([#192](https://github.com/n0-computer/iroh-gossip/issues/192)) - ([cf85954](https://github.com/n0-computer/iroh-gossip/commit/cf859543942b017d9e7034ecfa4aa61bfcf291e7))

### 🐛 Bug Fixes

- *(hyparview)* Retry a lost Join once, and skip a Join to a neighbor ([#217](https://github.com/n0-computer/iroh-gossip/issues/217)) - ([5cf0350](https://github.com/n0-computer/iroh-gossip/commit/5cf0350fa0951eea4cca9d4f660320565ac1f84f))
- *(net)* Don't close connection to buggy peers ([#189](https://github.com/n0-computer/iroh-gossip/issues/189)) - ([cd73d93](https://github.com/n0-computer/iroh-gossip/commit/cd73d93168207690e133caed3e20c5e26ebdddbc))
- *(net)* Close a connection on which neither peer sends ([#195](https://github.com/n0-computer/iroh-gossip/issues/195)) - ([5592b47](https://github.com/n0-computer/iroh-gossip/commit/5592b477236a0b3e7f9b956a03e905db1e79a38d))
- *(net)* Stop a busy publisher or a slow stream header from stalling the actor ([#186](https://github.com/n0-computer/iroh-gossip/issues/186)) - ([4348660](https://github.com/n0-computer/iroh-gossip/commit/4348660c226eb2321d728f2a9a8012c3c792e68e))
- *(net)* Keep a connection while streams are open ([#206](https://github.com/n0-computer/iroh-gossip/issues/206)) - ([c156ecb](https://github.com/n0-computer/iroh-gossip/commit/c156ecbf9bba9e8367df69e60e22c5bbaac3a905))
- *(net)* Cap the addresses that we keep per peer ([#215](https://github.com/n0-computer/iroh-gossip/issues/215)) - ([6d15773](https://github.com/n0-computer/iroh-gossip/commit/6d15773ab407cf619581cbf25721fb08c9a14999))
- *(proto)* Rejoin when alone, shuffle early, and test the paper claims ([#191](https://github.com/n0-computer/iroh-gossip/issues/191)) - ([dfeec14](https://github.com/n0-computer/iroh-gossip/commit/dfeec141b9041d2928b118a6e8d3afddc14a21f6))
- *(proto)* Log a duplicate message with its short hash ([#201](https://github.com/n0-computer/iroh-gossip/issues/201)) - ([44d1767](https://github.com/n0-computer/iroh-gossip/commit/44d17672baece0a1d840f771518f661b34ebb2c5))
- *(proto)* Keep a newly grafted peer more stable ([#207](https://github.com/n0-computer/iroh-gossip/issues/207)) - ([325cb1e](https://github.com/n0-computer/iroh-gossip/commit/325cb1e4ad861f10dc76d9dfd33b0636487524f7))
- *(proto)* Back off and limit refills of a short active view ([#196](https://github.com/n0-computer/iroh-gossip/issues/196)) - ([f2b1f29](https://github.com/n0-computer/iroh-gossip/commit/f2b1f29c48c36cfa853168c8e8b0c84ac9201da0))
- Stop the actor without a panic when the runtime shuts down ([#183](https://github.com/n0-computer/iroh-gossip/issues/183)) - ([b89fa98](https://github.com/n0-computer/iroh-gossip/commit/b89fa9816ece4f2fcd9b02a179f40c41509d4ada))
- Fix bugs in peer lifecycle management ([#161](https://github.com/n0-computer/iroh-gossip/issues/161)) - ([fcd81ae](https://github.com/n0-computer/iroh-gossip/commit/fcd81ae80b2cb0d64242b0b938304ca591d17b81))
- Connection lifecycle fixes ([#178](https://github.com/n0-computer/iroh-gossip/issues/178)) - ([f02f79e](https://github.com/n0-computer/iroh-gossip/commit/f02f79e857ab6b7cb697c4bbc08b9ad3c5ea6e11))
- Reject oversized messages at the API level ([#187](https://github.com/n0-computer/iroh-gossip/issues/187)) - ([6bb9fe0](https://github.com/n0-computer/iroh-gossip/commit/6bb9fe078396b9c96d029d6d63ab633318303354))

### 🚜 Refactor

- *(net)* Keep pending dials as a set ([#203](https://github.com/n0-computer/iroh-gossip/issues/203)) - ([7fd7494](https://github.com/n0-computer/iroh-gossip/commit/7fd74941507a701ceec1115bb7dbdba3fc4535cf))
- [**breaking**] Mark event and command enums and Metrics as non-exhaustive ([#198](https://github.com/n0-computer/iroh-gossip/issues/198)) - ([c5f44db](https://github.com/n0-computer/iroh-gossip/commit/c5f44db74ccc232d4e81523117b7e95e36d4c5c5))
- Don't maintain neighbor list twice ([#204](https://github.com/n0-computer/iroh-gossip/issues/204)) - ([056840d](https://github.com/n0-computer/iroh-gossip/commit/056840dd9de87051452966366af0f9e4b8fbf86b))
- Mark unreachable pub items as pub(crate) ([#222](https://github.com/n0-computer/iroh-gossip/issues/222)) - ([b4f40e1](https://github.com/n0-computer/iroh-gossip/commit/b4f40e112193aa659a06393597ced1cd76bc2834))

### 📚 Documentation

- *(examples)* Add nonce in chat example and fix nickname handling ([#200](https://github.com/n0-computer/iroh-gossip/issues/200)) - ([15af798](https://github.com/n0-computer/iroh-gossip/commit/15af798e5b3c1816b6f575640b9e8108978b35bf))
- Improve docs of public items ([#219](https://github.com/n0-computer/iroh-gossip/issues/219)) - ([1fe8899](https://github.com/n0-computer/iroh-gossip/commit/1fe8899454e48e804928f77625a13bbcc61cc02c))

### ⚡ Performance

- Use `FxHashMap` for some maps ([#220](https://github.com/n0-computer/iroh-gossip/issues/220)) - ([6e397a9](https://github.com/n0-computer/iroh-gossip/commit/6e397a932c96825871e1a397b8c730fa001e2a74))
- Reduce allocations ([#221](https://github.com/n0-computer/iroh-gossip/issues/221)) - ([0e06623](https://github.com/n0-computer/iroh-gossip/commit/0e066230f709d36e0705f44e4537a0c17b1be917))

### ⚙️ Miscellaneous Tasks

- *(secruity)* Hardening + lock files + dependabot cooldown ([#164](https://github.com/n0-computer/iroh-gossip/issues/164)) - ([282715a](https://github.com/n0-computer/iroh-gossip/commit/282715ade80dd0e74738a4be57ef735a7fc790bf))
- Run scheduled CI jobs earlier ([#153](https://github.com/n0-computer/iroh-gossip/issues/153)) - ([f128b98](https://github.com/n0-computer/iroh-gossip/commit/f128b98705f5ad3a28a92f1d2b483686348b6ae8))
- Bump all dependencies in lockfile ([#212](https://github.com/n0-computer/iroh-gossip/issues/212)) - ([32a6065](https://github.com/n0-computer/iroh-gossip/commit/32a60652671223ee01f312c7673a6bf37f54a801))

### Deps

- Bump deps ([#171](https://github.com/n0-computer/iroh-gossip/issues/171)) - ([2885dd9](https://github.com/n0-computer/iroh-gossip/commit/2885dd9f1120e46aca5acbcd7f212e4c64e278b9))
- Update rustls in lockfile ([#194](https://github.com/n0-computer/iroh-gossip/issues/194)) - ([6cbbafa](https://github.com/n0-computer/iroh-gossip/commit/6cbbafa865c1b60c813dd8f12f78a54a45c1ecb1))

## [0.101.0](https://github.com/n0-computer/iroh-gossip/compare/v0.100.0..0.101.0) - 2026-06-15

### ⛰️  Features

- *(deps)* [**breaking**] Update to iroh 1.0 ([#148](https://github.com/n0-computer/iroh-gossip/issues/148)) - ([c190a79](https://github.com/n0-computer/iroh-gossip/commit/c190a7902f8e215f5ceb8cf1680350d1a2ae4c9d))

## [0.100.0](https://github.com/n0-computer/iroh-gossip/compare/v0.99.0..0.100.0) - 2026-05-27

### ⛰️  Features

- [**breaking**] Update to iroh@1.0.0-rc.1 ([#144](https://github.com/n0-computer/iroh-gossip/issues/144)) - ([2712675](https://github.com/n0-computer/iroh-gossip/commit/2712675e3623f97f582be98029209ff73f3d5b1a))

## [0.99.0](https://github.com/n0-computer/iroh-gossip/compare/v0.98.0..0.99.0) - 2026-05-08

### ⛰️  Features

- [**breaking**] Update iroh and noq to 1.0-rc.0 ([#141](https://github.com/n0-computer/iroh-gossip/issues/141)) - ([8418636](https://github.com/n0-computer/iroh-gossip/commit/8418636b1e353bd28a2227a3d439fc4b509cedf8))

## [0.98.0](https://github.com/n0-computer/iroh-gossip/compare/v0.97.0..0.98.0) - 2026-04-20

### 🐛 Bug Fixes

- Use `n0_future::task::JoinSet` instead of `tokio::task::JoinSet` in `SendLoop` ([#138](https://github.com/n0-computer/iroh-gossip/issues/138)) - ([0dae56d](https://github.com/n0-computer/iroh-gossip/commit/0dae56de12d229930e72403fc62cd9e9bb99c5ae))
- Configure git identity in cleanup workflow ([#129](https://github.com/n0-computer/iroh-gossip/issues/129)) - ([a6cfbf7](https://github.com/n0-computer/iroh-gossip/commit/a6cfbf75e9b8f8b0178a3518469c1a3efab27c61))

### Deps

- Update to iroh 0.98 ([#132](https://github.com/n0-computer/iroh-gossip/issues/132)) - ([5076ea7](https://github.com/n0-computer/iroh-gossip/commit/5076ea721c614f508d60104264e78d815f95041c))

## [0.97.0](https://github.com/n0-computer/iroh-gossip/compare/v0.96.0..0.97.0) - 2026-03-16

### ⛰️  Features

- [**breaking**] Update to iroh@0.97 ([#130](https://github.com/n0-computer/iroh-gossip/issues/130)) - ([62b4805](https://github.com/n0-computer/iroh-gossip/commit/62b4805d8e9d6088d2fde9bb8bdb5590390b334b))

### 🐛 Bug Fixes

- Evict message id cache ([#128](https://github.com/n0-computer/iroh-gossip/issues/128)) - ([c1ad5da](https://github.com/n0-computer/iroh-gossip/commit/c1ad5da464e0238425b29be748478d027ea0b878))

## [0.96.0](https://github.com/n0-computer/iroh-gossip/compare/v0.95.0..0.96.0) - 2026-01-29

### ⛰️  Features

- Add `neighbors()` method to `GossipTopic` ([#124](https://github.com/n0-computer/iroh-gossip/issues/124)) - ([9e4ddaa](https://github.com/n0-computer/iroh-gossip/commit/9e4ddaa904c6e1b853081b9f2e4f9628ed274b08))

### 🐛 Bug Fixes

- Keep topic alive if either senders or receivers exist ([#119](https://github.com/n0-computer/iroh-gossip/issues/119)) - ([34b0e0e](https://github.com/n0-computer/iroh-gossip/commit/34b0e0ea87a2f0a6011d026bcffce78697689072))
- Clean-up connections after unexpected disconnects ([#117](https://github.com/n0-computer/iroh-gossip/issues/117)) - ([84f3945](https://github.com/n0-computer/iroh-gossip/commit/84f394577a4e8b52660a83dbbc955f0accb31b5a))

### 🧪 Testing

- Switch from tracing-test to n0-tracing-test ([#125](https://github.com/n0-computer/iroh-gossip/issues/125)) - ([48f522b](https://github.com/n0-computer/iroh-gossip/commit/48f522bb1504a7fbf4eb51112ea724fa1593a35f))

### ⚙️ Miscellaneous Tasks

- Ignore rustls-pemfile unmaintained advisory ([#122](https://github.com/n0-computer/iroh-gossip/issues/122)) - ([16c68bb](https://github.com/n0-computer/iroh-gossip/commit/16c68bb187c7127eb5694bfbbd1ec3da518bcc25))
- Upgrade to `iroh`v0.96 and the latest version of `iroh-quinn` ([#114](https://github.com/n0-computer/iroh-gossip/issues/114)) - ([13ef379](https://github.com/n0-computer/iroh-gossip/commit/13ef379f60e91f7292ae287051245db25ca8dd02))

## [0.95.0](https://github.com/n0-computer/iroh-gossip/compare/v0.94.0..0.95.0) - 2025-11-06

### Deps/refactor

- [**breaking**] Update to iroh main, port to n0-error ([#113](https://github.com/n0-computer/iroh-gossip/issues/113)) - ([4d2cb2f](https://github.com/n0-computer/iroh-gossip/commit/4d2cb2f3891e8dadd89a985fb6b5ad55d92e4c59))

## [0.94.0](https://github.com/n0-computer/iroh-gossip/compare/v0.93.1..0.94.0) - 2025-10-21

### 🚜 Refactor

- Use discovery service instead of `Endpoint::add_node_addr` ([#108](https://github.com/n0-computer/iroh-gossip/issues/108)) - ([f7e3ef4](https://github.com/n0-computer/iroh-gossip/commit/f7e3ef478a1c4f1ea934e29f3436582e68de734c))

### ⚙️ Miscellaneous Tasks

- Upgrade to iroh 0.94 ([#110](https://github.com/n0-computer/iroh-gossip/issues/110)) - ([ad78602](https://github.com/n0-computer/iroh-gossip/commit/ad78602a4bafad8db2a4264bf16fde12b08f7a5e))

## [0.93.1](https://github.com/n0-computer/iroh-gossip/compare/v0.93.0..0.93.1) - 2025-10-11

### ⚙️ Miscellaneous Tasks

- Update nightly version in CI and docs workflows ([#107](https://github.com/n0-computer/iroh-gossip/issues/107)) - ([b5e3414](https://github.com/n0-computer/iroh-gossip/commit/b5e3414f8db03910b6cea691bad69f798f1c34c6))

## [0.93.0](https://github.com/n0-computer/iroh-gossip/compare/v0.92.0..0.93.0) - 2025-10-09

### ⛰️  Features

- *(ci)* Add auto release on tag version push ([#93](https://github.com/n0-computer/iroh-gossip/issues/93)) - ([afa6e1d](https://github.com/n0-computer/iroh-gossip/commit/afa6e1dca9cef061642bece52fcdad5e877496e3))
- Set custom ALPN ([#92](https://github.com/n0-computer/iroh-gossip/issues/92)) - ([ff87b6a](https://github.com/n0-computer/iroh-gossip/commit/ff87b6a380a39c8274376ff26f874824ff80d752))

### ⚡ Performance

- Don't allocate in `Timers::wait_next` ([#102](https://github.com/n0-computer/iroh-gossip/issues/102)) - ([65278b7](https://github.com/n0-computer/iroh-gossip/commit/65278b75aa67cb6bed06c9770cacf701e952c0d3))

### ⚙️ Miscellaneous Tasks

- *(ci)* Fix url of the beta notification ([#94](https://github.com/n0-computer/iroh-gossip/issues/94)) - ([2021566](https://github.com/n0-computer/iroh-gossip/commit/20215660a71f192562477dafd677d5a974c6a7f3))
- Release prep ([#106](https://github.com/n0-computer/iroh-gossip/issues/106)) - ([099196e](https://github.com/n0-computer/iroh-gossip/commit/099196e72d806392ed609dfe36510b130839d52e))

## [0.92.0](https://github.com/n0-computer/iroh-gossip/compare/v0.91.0..0.92.0) - 2025-09-18

### ⚙️ Miscellaneous Tasks

- Upgrade `iroh`, `iroh-base`, `irpc` ([#91](https://github.com/n0-computer/iroh-gossip/issues/91)) - ([464fe69](https://github.com/n0-computer/iroh-gossip/commit/464fe69789ae8c8fefd7734a2f44db5aa447db26))

## [0.91.0](https://github.com/n0-computer/iroh-gossip/compare/v0.90.0..0.91.0) - 2025-07-31

### 🐛 Bug Fixes

- Make GossipSender `Clone` and GossipTopic `Sync` ([#81](https://github.com/n0-computer/iroh-gossip/issues/81)) - ([f215aa1](https://github.com/n0-computer/iroh-gossip/commit/f215aa13806425491cf328e42c611a0002da4371))

### 📚 Documentation

- Replace `iroh-net` mention in README ([#83](https://github.com/n0-computer/iroh-gossip/issues/83)) - ([e3df4ec](https://github.com/n0-computer/iroh-gossip/commit/e3df4ec7a56bcff0dafe6940d7d706ece5508891))

### ⚙️ Miscellaneous Tasks

- Add patch for `iroh` dependencies ([#82](https://github.com/n0-computer/iroh-gossip/issues/82)) - ([2e82a68](https://github.com/n0-computer/iroh-gossip/commit/2e82a683f93aa1ae50929da8ce95b23f85b466f1))
- [**breaking**] Prep for `v0.91.0` release ([#85](https://github.com/n0-computer/iroh-gossip/issues/85)) - ([d1fbfca](https://github.com/n0-computer/iroh-gossip/commit/d1fbfca15f484a41b8d6e8f771d14bf9fe5c7f81))

### Deps

- Update to irpc@main, iroh@main ([#84](https://github.com/n0-computer/iroh-gossip/issues/84)) - ([af7ae1f](https://github.com/n0-computer/iroh-gossip/commit/af7ae1f9bb9fa74aef97d510e062b09c03e96a87))

## [0.90.0](https://github.com/n0-computer/iroh-gossip/compare/v0.35.0..0.90.0) - 2025-06-27

### ⛰️  Features

- *(net)* Add shutdown function ([#69](https://github.com/n0-computer/iroh-gossip/issues/69)) - ([3cf2cd2](https://github.com/n0-computer/iroh-gossip/commit/3cf2cd2f3af5c79832b335b525b34db4290d0332))

### 🐛 Bug Fixes

- *(hyparview)* [**breaking**] Only add peers to active view after receiving neighbor messages ([#56](https://github.com/n0-computer/iroh-gossip/issues/56)) - ([5a441e6](https://github.com/n0-computer/iroh-gossip/commit/5a441e6cf5589fc3c7cf3c290005b1094895038c))
- *(hyparview)* Use shuffle replies as intended ([#57](https://github.com/n0-computer/iroh-gossip/issues/57)) - ([9632ced](https://github.com/n0-computer/iroh-gossip/commit/9632ced028ad7c211ac256b89d7ac0fcb32f55a6))
- *(hyparview)* Don't emit PeerData event for empty PeerData - ([c345f0a](https://github.com/n0-computer/iroh-gossip/commit/c345f0a0a6a099643f13a7a6743b308548a1c40e))
- *(plumtree)* Ensure eager relation is symmetrical - ([0abface](https://github.com/n0-computer/iroh-gossip/commit/0abface77c81dfde91c9f37da1f00bfee36f86d7))
- *(plumtree)* Clear graft timer to allow retry on new ihaves - ([b65cdce](https://github.com/n0-computer/iroh-gossip/commit/b65cdcea36c8b30ac3ab6443645c6e69795f6ebf))

### 🚜 Refactor

- *(hyparview)* Improve disconnect handling - ([5156d00](https://github.com/n0-computer/iroh-gossip/commit/5156d00f72be478872e7cecdf1b95d739b3b4fca))
- *(hyparview)* Remove obsolete parameter in hyparview - ([d954aa6](https://github.com/n0-computer/iroh-gossip/commit/d954aa62272d7f781ce762b42b06d2521e7d1b30))
- *(net)* [**breaking**] Remove `Joined` event, use `NeighborUp` ([#49](https://github.com/n0-computer/iroh-gossip/issues/49)) - ([c06f2ed](https://github.com/n0-computer/iroh-gossip/commit/c06f2ed64cb887d0714dfa1e75c0d66051c9d3e1))
- [**breaking**] Port to irpc, flatten event enum, remove cli impl ([#67](https://github.com/n0-computer/iroh-gossip/issues/67)) - ([a8d5cd2](https://github.com/n0-computer/iroh-gossip/commit/a8d5cd2b4c749993dd99f9d5eead073fd4b2498d))
- [**breaking**] Port to iroh@0.90 and n0-snafu ([#77](https://github.com/n0-computer/iroh-gossip/issues/77)) - ([1523227](https://github.com/n0-computer/iroh-gossip/commit/1523227c980c7d58efff805645aa50bea17402b0))
- [**breaking**] Change wire protocol to use uni streams per topic ([#75](https://github.com/n0-computer/iroh-gossip/issues/75)) - ([db1a135](https://github.com/n0-computer/iroh-gossip/commit/db1a13550d7b014e959fe807b45c3614e26e7105))

### 📚 Documentation

- Deny warnings for docs in CI ([#78](https://github.com/n0-computer/iroh-gossip/issues/78)) - ([b38b38f](https://github.com/n0-computer/iroh-gossip/commit/b38b38fc5970164a3c037b4d6306d8b7aee10f4f))

### 🧪 Testing

- Improve simulator ([#52](https://github.com/n0-computer/iroh-gossip/issues/52)) - ([8c30674](https://github.com/n0-computer/iroh-gossip/commit/8c306742c5823f8a6655252b1dbbbfb021c3400d))

### ⚙️ Miscellaneous Tasks

- Update clippy ([#79](https://github.com/n0-computer/iroh-gossip/issues/79)) - ([07b7b77](https://github.com/n0-computer/iroh-gossip/commit/07b7b77a8ceacad8094ec83209aa3d701a63d5b4))
- Upgrade to `iroh` at `0.90.0` and `irpc` at `0.5.0` ([#80](https://github.com/n0-computer/iroh-gossip/issues/80)) - ([0e613d8](https://github.com/n0-computer/iroh-gossip/commit/0e613d884e95203940d94b3b5363c972f4ef00d1))

### Change

- *(hyparview)* Send a ShuffleReply before disconnecting ([#59](https://github.com/n0-computer/iroh-gossip/issues/59)) - ([fd379fc](https://github.com/n0-computer/iroh-gossip/commit/fd379fc5f32ee52c2c7aad03c03c373c2ac69816))

## [0.35.0](https://github.com/n0-computer/iroh-gossip/compare/v0.34.1..0.35.0) - 2025-05-12

### 🐛 Bug Fixes

- Respect max message size when constructing IHave messages ([#63](https://github.com/n0-computer/iroh-gossip/issues/63)) - ([77c56f1](https://github.com/n0-computer/iroh-gossip/commit/77c56f1a769e561d1c8b91ebed6e02e7792bc2cb))

### 🚜 Refactor

- [**breaking**] Use new iroh-metrics version, no more global tracking ([#58](https://github.com/n0-computer/iroh-gossip/issues/58)) - ([2a37214](https://github.com/n0-computer/iroh-gossip/commit/2a372144b08f6db43f67536e8694659b4b326698))

### ⚙️ Miscellaneous Tasks

- Update dependencies ([#66](https://github.com/n0-computer/iroh-gossip/issues/66)) - ([dbec9b0](https://github.com/n0-computer/iroh-gossip/commit/dbec9b033cded5aa3e09b0c80d52bed697dfe880))
- Update to `iroh` v0.35 ([#68](https://github.com/n0-computer/iroh-gossip/issues/68)) - ([e6af27d](https://github.com/n0-computer/iroh-gossip/commit/e6af27d924db780e00b10017b18d4da3ef8db18a))

## [0.34.1](https://github.com/n0-computer/iroh-gossip/compare/v0.34.0..0.34.1) - 2025-03-24

### 🐛 Bug Fixes

- Allow instant reconnects, and always prefer newest connection ([#43](https://github.com/n0-computer/iroh-gossip/issues/43)) - ([ea1c773](https://github.com/n0-computer/iroh-gossip/commit/ea1c773659f88d7eed776b6b15cc0e559267afea))

## [0.34.0](https://github.com/n0-computer/iroh-gossip/compare/v0.33.0..0.34.0) - 2025-03-18

### 🐛 Bug Fixes

- Repo link for flaky tests ([#38](https://github.com/n0-computer/iroh-gossip/issues/38)) - ([0a03543](https://github.com/n0-computer/iroh-gossip/commit/0a03543db6aaedb7ac403e38360d5a1afc88b3f4))

### ⚙️ Miscellaneous Tasks

- Patch to use main branch of iroh dependencies ([#40](https://github.com/n0-computer/iroh-gossip/issues/40)) - ([d76305d](https://github.com/n0-computer/iroh-gossip/commit/d76305da7d75639638efcd537a1ffb13d07ef1ee))
- Update to latest iroh ([#42](https://github.com/n0-computer/iroh-gossip/issues/42)) - ([129e2e8](https://github.com/n0-computer/iroh-gossip/commit/129e2e80ec7a6efd29606fcdaf0202791a25778f))

## [0.33.0](https://github.com/n0-computer/iroh-gossip/compare/v0.32.0..0.33.0) - 2025-02-25

### ⛰️  Features

- Compile to wasm and run in browsers ([#37](https://github.com/n0-computer/iroh-gossip/issues/37)) - ([8f99f7d](https://github.com/n0-computer/iroh-gossip/commit/8f99f7d85fd8c410512b430a4ee2efd014828550))

### ⚙️ Miscellaneous Tasks

- Patch to use main branch of iroh dependencies ([#36](https://github.com/n0-computer/iroh-gossip/issues/36)) - ([7e16be8](https://github.com/n0-computer/iroh-gossip/commit/7e16be85dbf52af721aa8bb4c68723c029ce4bd2))
- Upgrade to latest `iroh` and `quic-rpc` ([#39](https://github.com/n0-computer/iroh-gossip/issues/39)) - ([a2ef813](https://github.com/n0-computer/iroh-gossip/commit/a2ef813c6033f1683162bb09d50f1f988f774cbe))

## [0.32.0](https://github.com/n0-computer/iroh-gossip/compare/v0.31.0..0.32.0) - 2025-02-04

### ⛰️  Features

- [**breaking**] Use explicit errors ([#34](https://github.com/n0-computer/iroh-gossip/issues/34)) - ([534f010](https://github.com/n0-computer/iroh-gossip/commit/534f01046332a21f6356d189c686f7c6c17af3c2))

### ⚙️ Miscellaneous Tasks

- Pin nextest version ([#29](https://github.com/n0-computer/iroh-gossip/issues/29)) - ([72b32d2](https://github.com/n0-computer/iroh-gossip/commit/72b32d25e8a810011456a2740581b3b3802f1cab))
- Remove individual repo project tracking ([#31](https://github.com/n0-computer/iroh-gossip/issues/31)) - ([8a79db6](https://github.com/n0-computer/iroh-gossip/commit/8a79db65a928ae0610d85301b009d3ec13b0fbe1))
- Update dependencies ([#35](https://github.com/n0-computer/iroh-gossip/issues/35)) - ([3c257a1](https://github.com/n0-computer/iroh-gossip/commit/3c257a1db9ea0ade0c35b060a28b1287321a532a))

## [0.31.0](https://github.com/n0-computer/iroh-gossip/compare/v0.30.1..0.31.0) - 2025-01-14

### ⚙️ Miscellaneous Tasks

- Add project tracking ([#28](https://github.com/n0-computer/iroh-gossip/issues/28)) - ([bf89c85](https://github.com/n0-computer/iroh-gossip/commit/bf89c85c3ffa78fea462d5ad7c7bae10f828d7b0))
- Upgrade to `iroh@v0.31.0` ([#30](https://github.com/n0-computer/iroh-gossip/issues/30)) - ([60f371e](https://github.com/n0-computer/iroh-gossip/commit/60f371ec61992889c390d64611e907a491812b96))

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


