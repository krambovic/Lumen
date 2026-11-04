# Exporting Lumen profiles

The Servers context menu contains **Export to Lumen** in both list and grid layouts.
The export form receives the selected configuration; it does not ask the user to paste raw server data.

Two separate choices are represented in version 3 of the `.lumen` envelope:

- `locked`: Lumen recipients can connect but the app does not offer edit, plain copy or re-export.
- `delivery`: `RECIPIENTS` uses RSA-OAEP-wrapped AES-256-GCM keys (up to 16 recipients);
  `PASSWORD` derives an AES key with PBKDF2-HMAC-SHA256, 600,000 iterations and a random salt;
  `ANYONE` is explicitly plaintext and offers no transport confidentiality.

Names, configuration contents and the lock flag are inside the authenticated payload
for encrypted delivery. No common application decryption secret is distributed in the APK.
Version 1 recipient-encrypted files and version 2 envelopes remain supported.
Early v2 password files using 210,000 iterations remain readable; other iteration
counts are rejected before derivation to prevent unbounded work from a file.

The encrypted configuration contains a versioned node snapshot with the original
source and normalized outbound, including protocol extensions, OpenVPN inline
materials, AUTO members and sing-box dependencies. It does not reconstruct the
configuration from a potentially lossy portable URI. Legacy link/config payloads
remain readable. The export/import file limit is 8 MiB, including envelope overhead;
larger files are rejected with an error, not silently truncated.

Password imports prompt before the ordinary import confirmation. They are rewrapped
for the local installation key before storage; the password is not persisted.
Locked public files are also encrypted with the local installation key on import.
Editable imports become ordinary server entries and can subsequently be shared or edited.

The recipient's public key is accessible from App settings and the export screen.
The private key remains in Android Keystore. Uninstalling Lumen loses access to files
encrypted for that installation; this implementation does not promise key migration.
Existing RSA keys are retained on update. New installations prefer RSA-3072 and fall
back to RSA-2048 on older Keymaster implementations. OAEP uses SHA-256 with MGF1
SHA-1 explicitly, matching Android Keystore compatibility requirements.

The saved configuration used for Quick Settings/widget starts is additionally
encrypted with a device-local AES-256-GCM Keystore key for closed profiles.
Authentication/key failures stop loading rather than fall back to an older config.
Old plaintext protected caches are migrated on read. Runtime processes still need
decrypted connection parameters in memory and private core working files.

This is app-level access control, not server-side revocation or protection against a
modified client/rooted device. Plaintext public delivery can be read outside Lumen.
Closed v3 profiles can have an absolute expiration measured from file creation,
Wi-Fi-only access and/or an unmetered-network requirement. Restrictions are inside
the authenticated encrypted payload. They are checked at import, connection startup
(including cached Quick Settings starts), runtime restarts and during active sessions.
Expired entries and their stored payloads are removed while Lumen runs or on its next
start; no background exact-alarm deletion is promised while the app is stopped.
Network violations disconnect the session without deleting an otherwise valid profile.
Persisted time observations and elapsed time make ordinary clock rollback harder,
but offline restrictions cannot resist a modified client, root, or clearing app data.
Public delivery has no authenticity or secrecy against a recipient who edits the file.
There is no server-side revocation, traffic quota, or guarantee of extraction resistance
on a compromised device.

## Verification

Regression coverage lives in `LumenProfileEnvelopeTest`, `LumenProfilePayloadTest`,
`ProtectedProfileCodecTest`, `ImportClassifierTest` and `LumenProfileDeviceTest`.
The first group covers protocol snapshots, wrong recipients/passwords, payload/IV
tampering, random encryption, compatibility, bounds and export restrictions.
Device instrumentation checks real Android Keystore operations, runtime resolution,
password rewrapping, encrypted cache migration and corrupted-cache rejection.
See `LUMEN_PROFILE_REVIEW.md` for the latest executed checks and their limitations.
