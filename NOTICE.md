# Notices

Bebra VPN / bebra-kvn is a fork of `youtubediscord/zapret-kvn`.

Upstream project:
https://github.com/youtubediscord/zapret-kvn

Fork repository:
https://github.com/krambovic/bebra-kvn

This repository keeps the MIT license notice for the upstream project and
adds copyright attribution for fork changes.

Bundled third-party components keep their own licenses and notices:

- Xray-core: https://github.com/XTLS/Xray-core
- sing-box: https://github.com/SagerNet/sing-box
- tun2socks: https://github.com/xjasonlyu/tun2socks
- zapret/WinDivert bundle: see files under `zapret/`, including
  `zapret/windivert.filter/manual.md`.

Optional external helpers downloaded at runtime:

- droute: https://github.com/snowluwu/droute
  GPL-3.0 component used as a separate helper for Discord TCP/UDP SOCKS5
  proxying via Discord-local DLL loading and Squirrel updater hook. Its source
  code is not embedded into this repository.
