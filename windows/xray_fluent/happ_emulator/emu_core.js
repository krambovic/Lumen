// emu_core: emulate liberror-code.so (ARM64) via unicorn.js to run the native
// crypt5 decrypt (jniGetErrorMessageFromString2). JS port of ANALYSIS/wasm/emu.c.
// Exports createDecryptor({MUnicorn, wrapperSrc, soBytes, keytable, verbose}) -> async -> {decrypt(inBytes)->Uint8Array}

const BASE=0x100000, HOOKBASE=0x40000000, HOOK_STOP=HOOKBASE+0x800;
const STACK_BASE=0x70000000, STACK_SIZE=4*1024*1024;
const HEAP_BASE=0x100000000, HEAP_SIZE=64*1024*1024;
const MMAP_BASE=0x200000000, MMAP_SIZE=32*1024*1024;
const TLS_BASE=0x300000000, TLS_SIZE=64*1024;
// JNI opaque handles
const H_CLASS=0x900001,H_MID=0x900002,H_INARR=0x900004,H_OUTARR=0x900005;
// reg ids (unicorn.js arm64)
const RX=i=> i<=28 ? 199+i : (i===29?1:2);  // X0..X28=199..227, X29=1, X30=2
const R_LR=2, R_SP=4, R_PC=260, R_TPIDR=262;

function createDecryptor(opts){
  const {MUnicorn, wrapperSrc, soBytes, keytable, verbose=0} = opts;
  return MUnicorn().then(M=>{
    new Function('Module', wrapperSrc)(M);
    return { decrypt: (inBytes)=> runOnce(M, wrapperSrc, soBytes, keytable, inBytes, verbose, opts) };
  });
}

function runOnce(M, wrapperSrc, soBytes, keytable, inBytes, verbose, opts={}){
  const uc = new M.Unicorn(M.ARCH_ARM64, M.MODE_LITTLE_ENDIAN);
  const log = verbose ? (...a)=>console.error(...a) : ()=>{};

  // ---- register helpers (values as JS Numbers; all our addrs < 2^53) ----
  const regGet = id => { const b=uc.reg_read(id,8);
    return b[0]+b[1]*256+b[2]*65536+b[3]*16777216 + (b[4]+b[5]*256+b[6]*65536+b[7]*16777216)*4294967296; };
  const regSet = (id,v)=>{ v=Number(v); const lo=v>>>0, hi=Math.floor(v/4294967296)>>>0;
    uc.reg_write(id,[lo&255,(lo>>8)&255,(lo>>16)&255,(lo>>24)&255, hi&255,(hi>>8)&255,(hi>>16)&255,(hi>>24)&255]); };
  const A = i => regGet(RX(i));
  const RET = v => regSet(RX(0), v);

  // ---- guest memory helpers ----
  const gwrite = (addr, bytes)=> uc.mem_write(addr, bytes);
  const gread  = (addr, n)=> uc.mem_read(addr, n);
  const gread32= addr => { const b=gread(addr,4); return (b[0]|b[1]<<8|b[2]<<16|b[3]<<24)>>>0; };
  const gwrite64=(addr,v)=>{ v=Number(v); const lo=v>>>0, hi=Math.floor(v/4294967296)>>>0;
    gwrite(addr,[lo&255,(lo>>8)&255,(lo>>16)&255,(lo>>24)&255, hi&255,(hi>>8)&255,(hi>>16)&255,(hi>>24)&255]); };
  const greadCStr = addr=>{ let out=[]; for(;;){ const chunk=gread(addr+out.length,64);
      for(let i=0;i<64;i++){ if(chunk[i]===0) return Uint8Array.from(out); out.push(chunk[i]); } } };
  const greadStr = addr => new TextDecoder().decode(greadCStr(addr));

  // ---- allocator (bump) ----
  let heapPtr = HEAP_BASE;
  const allocSizes = new Map();
  const gmalloc = n=>{ n=Number(n)||1; let p=Math.ceil(heapPtr/16)*16; allocSizes.set(p,n); heapPtr=p+n;
    if(heapPtr>HEAP_BASE+HEAP_SIZE) throw 'HEAP OOM'; return p; };
  const gsize = p=> allocSizes.get(p)||0;
  let mmapPtr = MMAP_BASE;
  const gmmap = n=>{ let p=Math.ceil(mmapPtr/4096)*4096; mmapPtr=p+Number(n); return p; };

  // ---- JNI state ----
  let inArrGuest=0, inLen=inBytes.length;
  let outLen=0, haveOut=false; const out=new Uint8Array(1<<20);
  let lastNewStr='';
  const mkJString = s=>{ const enc=new TextEncoder().encode(s); const p=gmalloc(enc.length+8);
    gwrite64(p, enc.length); gwrite(p+4, enc); gwrite(p+4+enc.length, [0]); return p; };
  const jsLen = h=> gread32(h);

  function getHelp(markerIn){
    const n=markerIn.length; let marker=''; for(let i=0;i<n;i++) marker+=markerIn[n-1-i];
    let M0=''; for(let i=0;i+1<n;i+=2) M0+=marker[i+1]; if(n%2===1) M0+=marker[n-1];
    const key=keytable[M0]; if(!key) log('[getHelp] NO MATCH M0='+M0); return key||'';
  }

  // ---- handler registry ----
  let redirect=-1;
  const handlers=[]; const hnames=[];
  const reg_hook=(name,fn)=>{ const i=handlers.length; handlers.push(fn); hnames.push(name); return HOOKBASE+i*4; };

  // libc
  const H={};
  H.malloc=()=>RET(gmalloc(A(0)));
  H.calloc=()=>{ const n=A(0)*A(1), p=gmalloc(n); gwrite(p, new Uint8Array(n)); RET(p); };
  H.realloc=()=>{ const o=A(0),n=A(1); if(!o){RET(gmalloc(n));return;} const os=gsize(o), p=gmalloc(n);
    gwrite(p, gread(o, Math.min(os,n))); RET(p); };
  H.free=()=>RET(0);
  H.posix_memalign=()=>{ const pp=A(0),al=A(1),n=A(2); let p=gmalloc(n+al); p=Math.ceil(p/al)*al; gwrite64(pp,p); RET(0); };
  H.memcpy=()=>{ const d=A(0),s=A(1),n=A(2); if(n) gwrite(d, gread(s,n)); RET(d); };
  H.memmove=H.memcpy;
  H.memset=()=>{ const d=A(0),c=A(1),n=A(2); if(n) gwrite(d, new Uint8Array(n).fill(c&255)); RET(d); };
  H.memcmp=()=>{ const a=gread(A(0),A(2)), b=gread(A(1),A(2)); for(let i=0;i<a.length;i++){ if(a[i]!==b[i]){ RET(a[i]<b[i]?-1:1); return; } } RET(0); };
  H.memchr=()=>{ const a=A(0),c=A(1)&255,n=A(2); const m=gread(a,n); for(let i=0;i<n;i++) if(m[i]===c){RET(a+i);return;} RET(0); };
  H.strlen=()=>RET(greadCStr(A(0)).length);
  H.strcmp=()=>{ const a=greadStr(A(0)), b=greadStr(A(1)); RET(a<b?-1:a>b?1:0); };
  H.strncmp=()=>{ const n=A(2); const a=greadStr(A(0)).slice(0,n), b=greadStr(A(1)).slice(0,n); RET(a<b?-1:a>b?1:0); };
  H.strcpy=()=>{ const d=A(0); const s=greadCStr(A(1)); gwrite(d,s); gwrite(d+s.length,[0]); RET(d); };
  H.strncpy=()=>{ const d=A(0),n=A(2); let s=greadCStr(A(1)); const buf=new Uint8Array(n); buf.set(s.slice(0,n)); gwrite(d,buf); RET(d); };
  H.strchr=()=>{ const a=A(0),c=A(1)&255; const s=greadCStr(a); const i=s.indexOf(c); RET(i<0?0:a+i); };
  H.strrchr=()=>{ const a=A(0),c=A(1)&255; const s=greadCStr(a); const i=s.lastIndexOf(c); RET(i<0?0:a+i); };
  H.strstr=()=>{ const a=A(0); const h=greadStr(a), nd=greadStr(A(1)); const i=h.indexOf(nd); RET(i<0?0:a+i); };
  H.strdup=()=>{ const s=greadCStr(A(0)); const p=gmalloc(s.length+1); gwrite(p,s); gwrite(p+s.length,[0]); RET(p); };
  H.strtol=()=>RET(parseInt(greadStr(A(0)), A(2)||10)|0);
  H.strtoul=()=>RET((parseInt(greadStr(A(0)), A(2)||10)>>>0));
  H.atoi=()=>RET(parseInt(greadStr(A(0)),10)|0);
  H.strcspn=()=>{ const s=greadStr(A(0)), set=greadStr(A(1)); let i=0; for(;i<s.length;i++) if(set.includes(s[i]))break; RET(i); };
  H.strspn=()=>{ const s=greadStr(A(0)), set=greadStr(A(1)); let i=0; for(;i<s.length;i++) if(!set.includes(s[i]))break; RET(i); };
  H.strpbrk=()=>{ const a=A(0); const s=greadStr(a), set=greadStr(A(1)); for(let i=0;i<s.length;i++) if(set.includes(s[i])){RET(a+i);return;} RET(0); };
  let errnoLoc=0;
  H.__errno=()=>RET(errnoLoc);
  H.stub0=()=>RET(0);
  H.time=()=>{ const t=1700000000; if(A(0)) gwrite64(A(0),t); RET(t); };
  H.clock_gettime=()=>{ const ts=A(1); if(ts){ gwrite64(ts,1700000000); gwrite64(ts+8,0);} RET(0); };
  H.gettimeofday=()=>{ const tv=A(0); if(tv){ gwrite64(tv,1700000000); gwrite64(tv+8,0);} RET(0); };
  let rng=0x12345678>>>0;
  const rnd=()=>{ rng=(Math.imul(rng,1103515245)+12345)>>>0; return rng; };
  H.rand=()=>RET((rnd()>>>16)&0x7fff);
  H.srand=()=>{ rng=A(0)>>>0; RET(0); };
  H.getentropy=()=>{ const n=A(1); const b=new Uint8Array(n); for(let i=0;i<n;i++) b[i]=(rnd()>>>16)&255; gwrite(A(0),b); RET(0); };
  H.getpid=()=>RET(1234);
  H.sysconf=()=>RET(4096);
  H.mmap=()=>RET(gmmap(A(1)));
  H.abort=()=>{ log('[guest abort]'); uc.emu_stop(); };
  H.__stack_chk_fail=()=>{ log('[stack_chk_fail]'); uc.emu_stop(); };
  H.__system_property_get=()=>{ if(A(1)) gwrite(A(1),[0]); RET(0); };
  H.getenv=()=>RET(0);
  H.getauxval=()=>{ const t=A(0); RET(t===16?0x2 : t===6?4096 : 0); };  // HWCAP=NEON only (no crypto-ext)
  H.pthread_self=()=>RET(1);
  H.syscall=()=>{ const n=A(0); if(n===278){ const buf=A(1),len=A(2); const b=new Uint8Array(len); for(let i=0;i<len;i++) b[i]=(rnd()>>>16)&255; gwrite(buf,b); RET(len); return; } RET(0); };
  H.snprintf=()=>{ if(A(1)) gwrite(A(0),[0]); RET(0); };
  // TLS
  const tlsVals=new Map(); let tlsNext=1;
  H.pthread_key_create=()=>{ const k=tlsNext++; if(A(0)) gwrite(A(0), [k&255,(k>>8)&255,(k>>16)&255,(k>>24)&255]); RET(0); }; // pthread_key_t = 4 bytes
  H.pthread_key_delete=()=>RET(0);
  H.pthread_setspecific=()=>{ tlsVals.set(A(0), A(1)); RET(0); };
  H.pthread_getspecific=()=>RET(tlsVals.get(A(0))||0);
  H.pthread_once=()=>{ const ctrl=A(0), init=A(1); RET(0); if(gread32(ctrl)===0){ gwrite(ctrl,[1,0,0,0]); redirect=init; } };

  // JNI
  const J={};
  J.FindClass=()=>RET(H_CLASS); J.GetObjectClass=()=>RET(H_CLASS);
  J.GetMethodID=()=>RET(H_MID); J.GetStaticMethodID=()=>RET(H_MID);
  J.NewStringUTF=()=>{ lastNewStr=greadStr(A(1)); RET(mkJString(lastNewStr)); };
  J.CallObjectMethodV=()=>RET(H_INARR);
  J.CallStaticObjectMethodV=()=>{ const key=getHelp(lastNewStr); log('[getHelp] marker='+lastNewStr+' keylen='+key.length); RET(mkJString(key)); };
  J.GetStringUTFChars=()=>{ if(A(2)) gwrite(A(2),[0,0,0,0]); RET(A(1)+4); };
  J.GetStringUTFLength=()=>RET(jsLen(A(1)));
  J.ReleaseStringUTFChars=()=>RET(0);
  J.GetArrayLength=()=>RET(A(1)===H_INARR?inLen:outLen);
  J.GetByteArrayElements=()=>{ if(A(2)) gwrite(A(2),[0,0,0,0]); RET(inArrGuest); };
  J.ReleaseByteArrayElements=()=>RET(0);
  J.DeleteLocalRef=()=>RET(0);
  J.NewByteArray=()=>{ outLen=A(1); RET(H_OUTARR); };
  J.SetByteArrayRegion=()=>{ const start=A(2),len=A(3),buf=A(4); if(start+len<=out.length){ out.set(gread(buf,len), start); haveOut=true; } RET(0); };
  J.ExceptionCheck=()=>RET(0); J.ExceptionOccurred=()=>RET(0); J.ExceptionClear=()=>RET(0);
  J.ThrowNew=()=>{ log('[ThrowNew] '+(A(2)?greadStr(A(2)):'')); RET(0); };

  function resolveImport(name){
    if(H[name]) return reg_hook(name, H[name]);
    return reg_hook(name, H.stub0);
  }

  // ---- ELF load (into JS buffer, then one mem_write) ----
  const dv=new DataView(soBytes.buffer, soBytes.byteOffset, soBytes.byteLength);
  const u32=o=>dv.getUint32(o,true), u16=o=>dv.getUint16(o,true), u64=o=>dv.getUint32(o,true)+dv.getUint32(o+4,true)*4294967296;
  const e_phoff=u64(0x20), e_phnum=u16(0x38);
  let maxv=0; const loads=[];
  for(let i=0;i<e_phnum;i++){ const p=e_phoff+i*56; const type=u32(p);
    if(type===1){ const off=u64(p+8),va=u64(p+16),fsz=u64(p+32),msz=u64(p+40); loads.push({off,va,fsz}); if(va+msz>maxv)maxv=va+msz; } }
  const span=Math.ceil((maxv+0xffff)/0x10000)*0x10000;
  const sobk=new Uint8Array(span);
  for(const L of loads) sobk.set(soBytes.subarray(L.off, L.off+L.fsz), L.va);
  const sdv=new DataView(sobk.buffer);
  const s32=o=>sdv.getUint32(o,true), s64=o=>sdv.getUint32(o,true)+sdv.getUint32(o+4,true)*4294967296;
  const sset64=(o,v)=>{ v=Number(v); sdv.setUint32(o, v>>>0, true); sdv.setUint32(o+4, Math.floor(v/4294967296)>>>0, true); };

  // dynamic
  let dynVa=0; for(let i=0;i<e_phnum;i++){ const p=e_phoff+i*56; if(u32(p)===2) dynVa=u64(p+16); }
  let rela=0,relasz=0,jmprel=0,pltsz=0,symtab=0,strtab=0,syment=24,initarr=0,initarrsz=0;
  for(let d=dynVa; ; d+=16){ const tag=s64(d), val=s64(d+8); if(tag===0)break;
    if(tag===7)rela=val; else if(tag===8)relasz=val; else if(tag===23)jmprel=val; else if(tag===2)pltsz=val;
    else if(tag===6)symtab=val; else if(tag===5)strtab=val; else if(tag===11)syment=val;
    else if(tag===25)initarr=val; else if(tag===27)initarrsz=val; }
  const symName=idx=>{ const nameOff=s32(symtab+idx*syment); let s=''; for(let i=strtab+nameOff;;i++){ const c=sobk[i]; if(!c)break; s+=String.fromCharCode(c); } return s; };
  const symShndx=idx=> sdv.getUint16(symtab+idx*syment+6, true);
  const symValue=idx=> s64(symtab+idx*syment+8);
  function applyRelocs(r, sz){ for(let o=0;o<sz;o+=24){ const off=s64(r+o), info_lo=s32(r+o+8), info_hi=s32(r+o+12), add=s64(r+o+16);
    const type=info_lo, symi=info_hi;
    if(type===1027){ sset64(off, BASE+add); } // RELATIVE
    else if(type===1026||type===1025||type===257){ // JUMP_SLOT/GLOB_DAT/ABS64
      if(symShndx(symi)) sset64(off, BASE+symValue(symi)+(type===257?add:0));
      else sset64(off, resolveImport(symName(symi)));
    } } }
  applyRelocs(rela, relasz); applyRelocs(jmprel, pltsz);

  // errno + input buffer
  errnoLoc=gmalloc(8);
  inArrGuest=gmalloc(inLen);

  // env table
  const table=gmalloc(0x800);
  const tbuf=new Uint8Array(0x800);
  const tdv=new DataView(tbuf.buffer);
  const JSET=(off,name,fn)=>{ const a=reg_hook(name,fn); tdv.setUint32(off, a>>>0, true); tdv.setUint32(off+4, Math.floor(a/4294967296)>>>0, true); };
  JSET(0x30,'FindClass',J.FindClass); JSET(0x88,'ExceptionClear',J.ExceptionClear);
  JSET(0xb8,'DeleteLocalRef',J.DeleteLocalRef); JSET(0xf8,'GetObjectClass',J.GetObjectClass);
  JSET(0x108,'GetMethodID',J.GetMethodID); JSET(0x118,'CallObjectMethodV',J.CallObjectMethodV);
  JSET(0x388,'GetStaticMethodID',J.GetStaticMethodID); JSET(0x398,'CallStaticObjectMethodV',J.CallStaticObjectMethodV);
  JSET(0x538,'NewStringUTF',J.NewStringUTF); JSET(0x540,'GetStringUTFLength',J.GetStringUTFLength);
  JSET(0x548,'GetStringUTFChars',J.GetStringUTFChars); JSET(0x550,'ReleaseStringUTFChars',J.ReleaseStringUTFChars);
  JSET(0x558,'GetArrayLength',J.GetArrayLength); JSET(0x580,'NewByteArray',J.NewByteArray);
  JSET(0x5c0,'GetByteArrayElements',J.GetByteArrayElements); JSET(0x600,'ReleaseByteArrayElements',J.ReleaseByteArrayElements);
  JSET(0x680,'SetByteArrayRegion',J.SetByteArrayRegion); JSET(0x720,'ExceptionCheck',J.ExceptionCheck);
  JSET(0x78,'ExceptionOccurred',J.ExceptionOccurred); JSET(0x35c,'ThrowNew',J.ThrowNew);
  const envp=gmalloc(8);

  // ---- map memory ----
  uc.mem_map(BASE, span, M.PROT_ALL);
  uc.mem_map(STACK_BASE, STACK_SIZE, M.PROT_ALL);
  uc.mem_map(HEAP_BASE, HEAP_SIZE, M.PROT_ALL);
  uc.mem_map(MMAP_BASE, MMAP_SIZE, M.PROT_ALL);
  uc.mem_map(TLS_BASE, TLS_SIZE, M.PROT_ALL);
  // hook page (filled with RET = 0xd65f03c0)
  const hookpage=new Uint8Array(0x1000); for(let i=0;i<0x1000;i+=4){ hookpage[i]=0xc0;hookpage[i+1]=0x03;hookpage[i+2]=0x5f;hookpage[i+3]=0xd6; }
  uc.mem_map(HOOKBASE, 0x1000, M.PROT_ALL);
  uc.mem_write(HOOKBASE, hookpage);
  // write loaded .so, env table, input
  uc.mem_write(BASE, sobk);
  uc.mem_write(table, tbuf);
  gwrite64(envp, table);
  gwrite64(errnoLoc, 0);
  gwrite(inArrGuest, inBytes);
  regSet(R_TPIDR, TLS_BASE+0x1000);
  gwrite64(TLS_BASE+0x28, 0); // canary slot (value unused by decrypt)
  regSet(R_SP, STACK_BASE+STACK_SIZE-0x100);  // SP must be valid for init_array too

  // ---- dispatch hook: native handlers live in the hook page; HOOK_CODE redirects PC ----
  uc.hook_add(M.HOOK_CODE, (handle, address, size, ud)=>{
    const addr=Number(address);
    if(addr>=HOOKBASE && addr<HOOKBASE+0x800){
      const idx=(addr-HOOKBASE)/4;
      if(idx<handlers.length){ redirect=-1; handlers[idx]();
        const tgt = redirect>=0 ? redirect : regGet(R_LR); redirect=-1; regSet(R_PC, tgt); }
    }
  }, null, HOOKBASE, HOOKBASE+0x800);

  // ---- fast path: intercept the dominant modular exponentiation ----
  // The RSA decrypt spends ~200M of ~217M guest instructions in the constant-
  // time 2048-bit CRT modexp. We replace it with JS BigInt: read the (base,
  // exp, modulus) BIGNUMs at the function entry, compute base^exp mod modulus,
  // write it into the result BIGNUM, and return. A strict guard validates the
  // argument shape; if it doesn't match (e.g. an updated .so), we leave the
  // instruction untouched and the routine runs natively (slower, never wrong).
  const sgread32 = a=>{ const b=gread(a,4); return (b[0]|b[1]<<8|b[2]<<16|b[3]<<24)|0; };
  const gread64v = a=>{ const b=gread(a,8); return b[0]+b[1]*256+b[2]*65536+b[3]*16777216+(b[4]+b[5]*256+b[6]*65536+b[7]*16777216)*4294967296; };
  const inHeapRange = p=> p>=HEAP_BASE && p<HEAP_BASE+HEAP_SIZE;
  function bnRead(p){ // -> {val:BigInt, top, dmax, neg} | null
    if(!inHeapRange(p) || (p&7)) return null;
    const dptr=gread64v(p), top=sgread32(p+8), dmax=sgread32(p+12), neg=sgread32(p+16);
    if(top<0||top>256||dmax<top||neg<0||neg>1) return null;
    if(top===0) return {val:0n, top:0, dmax, neg};
    if(!inHeapRange(dptr)) return null;
    const limbs=gread(dptr, top*8); let val=0n;
    for(let i=top-1;i>=0;i--){ let limb=0n; for(let b=7;b>=0;b--) limb=(limb<<8n)|BigInt(limbs[i*8+b]); val=(val<<64n)|limb; }
    return {val, top, dmax, neg};
  }
  function bnWrite(p, val){ // write nonneg BigInt into BIGNUM at p (fresh d buffer)
    let v=val<0n?-val:val;
    const bytes=[]; while(v>0n){ bytes.push(Number(v&0xffn)); v>>=8n; }
    while(bytes.length%8) bytes.push(0);          // pad up to a whole 64-bit limb
    if(bytes.length===0) for(let i=0;i<8;i++) bytes.push(0);
    const cap=bytes.length/8;
    let top=cap; while(top>0){ let z=true; for(let b=0;b<8;b++) if(bytes[(top-1)*8+b]){z=false;break;} if(z) top--; else break; }
    const d=gmalloc(cap*8); gwrite(d, Uint8Array.from(bytes));
    gwrite64(p, d);                               // rr->d
    gwrite(p+8,  [top&255,(top>>8)&255,(top>>16)&255,(top>>24)&255]);   // top
    gwrite(p+12, [cap&255,(cap>>8)&255,(cap>>16)&255,(cap>>24)&255]);   // dmax
    gwrite(p+16, [0,0,0,0]);                                            // neg = 0
  }
  function bnModPow(b,e,m){ if(m===1n) return 0n; b%=m; if(b<0n) b+=m; let r=1n; while(e>0n){ if(e&1n) r=(r*b)%m; e>>=1n; b=(b*b)%m; } return r; }

  // Entry of the generic Montgomery modexp BN_mod_exp_mont in this liberror-code.so
  // (statically-linked, stripped OpenSSL — located by profiling, see ANALYSIS).
  // We verify the prologue before trusting the offset so a future/updated .so
  // can't be silently mis-patched: on mismatch we run the lib unmodified.
  const MODEXP_OFF=0x1c5ef8, MODEXP_PROLOGUE=[0xa9ba7bfd,0xa9016ffc,0xa90267fa];
  let modexpSkips=0;
  let MA = opts.modexpAddr ? BASE+opts.modexpAddr : 0;
  if(!MA && !opts.noFastPath){
    const b=gread(BASE+MODEXP_OFF,12); let ok=true;
    for(let i=0;i<3;i++){ const w=(b[i*4]|b[i*4+1]<<8|b[i*4+2]<<16|b[i*4+3]<<24)>>>0; if(w!==MODEXP_PROLOGUE[i]) ok=false; }
    if(ok) MA=BASE+MODEXP_OFF; else log('[fastpath] modexp prologue mismatch — running full native emulation');
  }
  if(MA){
    uc.hook_add(M.HOOK_CODE, (h,address)=>{
      if(Number(address)!==MA) return;
      // BN_mod_exp_mont(rr=X0, a=X1, p=X2, m=X3, ...): rr = a^p mod m.
      // Guard each call so we only replace genuine modexp triples; anything
      // else falls through and executes natively (correct, just slower).
      const rr=A(0), a=A(1), p=A(2), m=A(3);
      const A_=bnRead(a), P_=bnRead(p), M_=bnRead(m), R_=bnRead(rr);
      if(!A_||!P_||!M_||!R_) return;
      if(M_.top<16 || M_.neg || A_.neg || P_.neg) return;       // ≥1024-bit, non-negative
      if(P_.top<1 || P_.top>M_.top+1 || A_.top>M_.top+1) return;
      if((M_.val&1n)===0n) return;                              // RSA modulus is odd
      bnWrite(rr, bnModPow(A_.val, P_.val, M_.val));
      RET(1);                                                   // BN_mod_exp_mont returns 1 on success
      regSet(R_PC, regGet(R_LR));
      modexpSkips++;
    }, null, MA, MA+4);
  }
  opts._skips = ()=>modexpSkips;

  // ---- run init_array ----
  if(initarr) for(let o=0;o<initarrsz;o+=8){ const fn=s64(initarr+o); if(!fn)continue;
    regSet(R_LR, HOOK_STOP); try{ uc.emu_start(fn, HOOK_STOP, 0, 0); }catch(e){ log('[init err] '+e); }
  }

  // resolve entry
  const ENTRY='Java_su_happ_proxyutility_util_ErrorCodeJNIWrapper_jniGetErrorMessageFromString2';
  let entry=0;
  for(let so=symtab; so<strtab; so+=syment){ const shndx=sdv.getUint16(so+6,true); const nameOff=s32(so);
    if(shndx && nameOff){ let s=''; for(let i=strtab+nameOff;;i++){ const c=sobk[i]; if(!c)break; s+=String.fromCharCode(c); }
      if(s===ENTRY){ entry=BASE+s64(so+8); break; } } }
  if(!entry) throw 'entry symbol not found';
  log('[entry] file 0x'+(entry-BASE).toString(16)+' inLen='+inLen);

  // call entry(env, thiz=1, inarr=H_INARR)
  regSet(RX(0), envp); regSet(RX(1), 1); regSet(RX(2), H_INARR);
  regSet(R_SP, STACK_BASE+STACK_SIZE-0x100);
  regSet(R_LR, HOOK_STOP);
  try{ uc.emu_start(entry, HOOK_STOP, 0, 0); }
  catch(e){ log('[emu_start error] '+e); }
  log('[done] haveOut='+haveOut+' outLen='+outLen);
  uc.close();
  return out.slice(0, outLen);
}

export { createDecryptor };
