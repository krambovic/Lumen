const fs = require('fs');
const path = require('path');

const link = process.argv[2];
if (!link) {
  console.error("Usage: node decrypt_cli.js <happ_link>");
  process.exit(1);
}

const prefix = "happ://crypt5/";
if (!link.startsWith(prefix)) {
  console.error("Error: Not a crypt5 link");
  process.exit(1);
}

const payload = link.slice(prefix.length);

function m4831f(s) {
  const full = s.length - (s.length % 6);
  let out = '';
  for (let i = 0; i < full; i += 6) {
    const b = s.slice(i, i + 6);
    out += b[1] + b[3] + b[5] + b[0] + b[2] + b[4];
  }
  return out + s.slice(full);
}

function swapPairs(s) {
  const arr = [...s];
  for (let i = 0; i + 1 < arr.length; i += 2) [arr[i], arr[i + 1]] = [arr[i + 1], arr[i]];
  return arr.join('');
}

function b64DecodeUrlSafe(s) {
  s = s.replace(/-/g, '+').replace(/_/g, '/');
  while (s.length % 4) s += '=';
  return Buffer.from(s, 'base64');
}

const assetDir = __dirname;
const wrapperSrc = fs.readFileSync(path.join(assetDir, 'unicorn-wrapper.js'), 'utf8');
const emuCoreSrc = fs.readFileSync(path.join(assetDir, 'emu_core.js'), 'utf8');
const soBytes = new Uint8Array(fs.readFileSync(path.join(assetDir, 'liberror-code.so')));
const keytable = JSON.parse(fs.readFileSync(path.join(assetDir, 'keytable.json'), 'utf8'));
const MUnicorn = require(path.join(assetDir, 'unicorn_aarch64.js'));

const nativeIn = m4831f(payload);
const payloadBytes = new TextEncoder().encode(nativeIn);

const emuScope = {};
const emuFunc = new Function('exports', emuCoreSrc.replace(/export\s*\{\s*createDecryptor\s*\};?/g, '') + "\nexports.createDecryptor = createDecryptor;");
emuFunc(emuScope);

emuScope.createDecryptor({
    MUnicorn: MUnicorn,
    wrapperSrc: wrapperSrc,
    soBytes: soBytes,
    keytable: keytable,
    verbose: 0
}).then(decryptor => {
    try {
        const outBytes = decryptor.decrypt(payloadBytes);
        if (outBytes.length > 0) {
            const obfuscated = new TextDecoder().decode(outBytes);
            const decoded = b64DecodeUrlSafe(swapPairs(obfuscated));
            console.log(new TextDecoder().decode(decoded).trim());
            process.exit(0);
        } else {
            console.error("Error: Decryption failed (empty output)");
            process.exit(1);
        }
    } catch(e) {
        console.error("Error during emulation:", e.message || e);
        process.exit(1);
    }
}).catch(err => {
    console.error("Error during initialization:", err.message || err);
    process.exit(1);
});
