# sing-box extended compatibility

Current base: `v1.14.3-extended-2.7.3`, commit
`d9442d14b19ddd02605bf3ee893a171947038e19`.

The shared Android/Desktop MASQUE patch remains necessary. Upstream added L4
proxy mode, endpoint `address`/`port`, and handshake timeout, but does not accept
Lumen's direct `server`, `server_port`, `private_key`, `public_key`, and
`tunnel_address` profiles. The rebased patch preserves these profiles, DER public
keys, and IPv4-only/IPv6-only tunnel addresses while retaining the new upstream
options. Both build scripts apply the patch before compiling.

The former `sing-box-extended-interface-monitor.patch` is no longer applied.
All four guards are present upstream: DHCP auto-interface setup, the direct
outbound's own-address lookup, the default-interface rule, and the
network-interface-address rule. The old patch is kept as the audit reference;
applying it again would fail or duplicate the upstream fix.

`android/app/singbox-core.json` records the base commit and SHA-256 of each ABI.
