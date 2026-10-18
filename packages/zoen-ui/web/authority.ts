import {bytes,integer,object,publicKey} from './boundary.ts';

export type SealedAuthority={version:1;identity:string;device:string;iv:Uint8Array;ciphertext:Uint8Array};
const aad=(identity:string,device:string)=>new TextEncoder().encode(`zoen-browser-authority-v1\0${device}\0${identity}`);

export function parseAuthority(value:unknown):SealedAuthority{
  const record=object(value);const version=integer(record.version);const iv=bytes(record.iv);const ciphertext=bytes(record.ciphertext);
  if(version!==1||iv.length!==12||ciphertext.length!==48)throw new Error('A autoridade deste navegador está inválida.');
  return {version,identity:publicKey(record.identity),device:publicKey(record.device),iv,ciphertext};
}
export function authorityJson(value:SealedAuthority):string{return JSON.stringify({...value,iv:Array.from(value.iv),ciphertext:Array.from(value.ciphertext)});}

/** Called only in the worker. Clear the temporary Rust export after encryption. */
export async function sealAuthority(seed:Uint8Array,key:CryptoKey,identity:string,device:string):Promise<SealedAuthority>{
  const plain=Uint8Array.from(seed);
  try{
    if(seed.length!==32)throw new Error('Chave de conta inválida.');
    const iv=crypto.getRandomValues(new Uint8Array(12));
    const ciphertext=new Uint8Array(await crypto.subtle.encrypt({name:'AES-GCM',iv,additionalData:aad(identity,device)},key,plain));
    return {version:1,identity,device,iv,ciphertext};
  }finally{seed.fill(0);plain.fill(0);}
}

/** The caller passes its result directly to Rust and clears it in a finally block. */
export async function openAuthority(value:unknown,key:CryptoKey,device:string):Promise<Uint8Array>{
  const record=parseAuthority(value);if(record.device!==device)throw new Error('Esta conta pertence a outro dispositivo.');
  const seed=new Uint8Array(await crypto.subtle.decrypt({name:'AES-GCM',iv:Uint8Array.from(record.iv),additionalData:aad(record.identity,device)},key,Uint8Array.from(record.ciphertext)));
  if(seed.length!==32){seed.fill(0);throw new Error('Chave de conta inválida.');}return seed;
}
