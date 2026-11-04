# Protected .lumen profiles (v1)

The integrated UI writes v3 envelopes: multiple recipient keys, password delivery,
explicit public delivery, absolute expiration and network restrictions.
The default for a closed profile is recipient encryption. Password imports are
re-encrypted for the local installation before storage. Public delivery is
unencrypted distribution with UI restrictions only; use recipient/password
delivery when the file contents must remain confidential. Legacy v1 files
remain readable. The description below covers the recipient-bound v1 path.

Open App settings → My recipient key. A recipient copies
their installation's public key and sends it to the profile creator. The
creator opens an existing server's context menu → Export to Lumen, chooses
recipient/password/public delivery and saves a `.lumen` file. The source
configuration is loaded from that server automatically. Import it through Lumen's existing file import or Android
Open with Lumen flow.

The file is an authenticated AES-256-GCM envelope with a fresh key and IV. The
AES key is wrapped with RSA-OAEP SHA-256 / MGF1 SHA-1 for the recipient's Android
Keystore key. No universal secret is embedded in the app. A wrong recipient,
changed ciphertext, changed IV or unsupported version fails closed. This
format does not claim to implement or import proprietary `.npv` files.

The database stores a reference and display name. The encrypted envelope lives
once in private no-backup storage; server address, port, raw URI and normalized
outbound are not placed in the row. Decryption happens for runtime use. The UI
and view model block editing, plaintext copying, QR export and group export of
protected records. Verbose core diagnostics are suppressed for these sessions.

Uninstalling the app loses the recipient key and access to these files. This
protects distribution and normal app workflows, not a rooted or instrumented
device: a VPN client must eventually use its connection parameters in memory.
The public key identifies a recipient, not the author of an envelope; v1 does
not assert issuer identity, revocation, expiration or online license checks.

Current v3 implementation and verification details: `docs/LUMEN_PROFILE_EXPORT.md`
and `docs/LUMEN_PROFILE_REVIEW.md`.
