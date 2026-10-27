import {readFile,readdir,writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
export const ORIGIN='https://zenoui.test';
export const digest=bytes=>createHash('sha256').update(bytes).digest('hex');
const types={'.html':'text/html; charset=utf-8','.js':'text/javascript; charset=utf-8','.css':'text/css; charset=utf-8','.json':'application/json','.wasm':'application/wasm'};
export async function manifestFor(root) {
  const entries=[];
  async function walk(dir){for(const entry of (await readdir(dir,{withFileTypes:true})).sort((a,b)=>a.name.localeCompare(b.name))){const file=path.join(dir,entry.name);if(entry.isDirectory())await walk(file);else if(entry.isFile()){
    const relative=path.relative(root,file).split(path.sep).join('/'),bytes=await readFile(file);
    entries.push({url:`${ORIGIN}/ZenoFCIS/${relative}`,file:relative,size:bytes.length,sha256:digest(bytes),type:types[path.extname(file)]??'application/octet-stream'});
  }}}
  await walk(root);return {format:'zenoui/asset-manifest/1',origin:ORIGIN,sha256:digest(JSON.stringify(entries)),entries};
}
export async function loadAssets(root,manifest) {
  if(manifest.format!=='zenoui/asset-manifest/1'||manifest.origin!==ORIGIN||digest(JSON.stringify(manifest.entries))!==manifest.sha256)throw new Error('Invalid asset manifest');
  const assets=new Map();
  for(const entry of manifest.entries){
    const file=path.resolve(root,entry.file);
    if(!file.startsWith(path.resolve(root)+path.sep)||new URL(entry.url).origin!==ORIGIN||assets.has(entry.url))throw new Error('Invalid asset route');
    const body=await readFile(file);
    if(body.length!==entry.size||digest(body)!==entry.sha256)throw new Error(`Asset hash mismatch: ${entry.file}`);
    assets.set(entry.url,{...entry,body});
  }
  return assets;
}
if(process.argv[1]===fileURLToPath(import.meta.url)){
 const [root,output]=process.argv.slice(2);if(!root||!output)throw new Error('usage: asset-manifest.mjs PUBLIC MANIFEST');
 const manifest=await manifestFor(root);await writeFile(output,JSON.stringify(manifest,null,2)+'\n');
 console.log(`${manifest.entries.length} assets; manifest SHA-256 ${manifest.sha256}`);
}
