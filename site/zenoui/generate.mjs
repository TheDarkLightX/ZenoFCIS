// Deterministic offline generator: validated data and a closed shared asset set.
import {readFile,writeFile,mkdir,copyFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {describe,canonical,layoutCheck} from '../public/zenoui/app-contract.js';
const [rawPath,metaPath,out,runtime]=process.argv.slice(2);
if(!runtime)throw Error('usage: generate.mjs RAW METADATA OUT RUNTIME');
const raw=JSON.parse(await readFile(rawPath,'utf8')),meta=JSON.parse(await readFile(metaPath,'utf8'));
const description=describe(raw.descriptor,meta);layoutCheck(description.layout);
if(BigInt(description.domain??'2000001')>2000000n)throw Error('UiDomainTooLarge');
if(Buffer.byteLength(JSON.stringify(raw.source),'utf8')>65536)throw Error('UiPreparationLimit');
if(Array.from(JSON.stringify(raw.source)).length>31000)throw Error('UiPreparationRequests');
const content={description,source:raw.source},contract=createHash('sha256').update(canonical(content)).digest('hex');
const bundle={...content,contract};
const publicDir=fileURLToPath(new URL('../public/',import.meta.url));
const shared=['demo-module.js','site.css','zenoui/app-contract.js','zenoui/app-state.js','zenoui/components.js','zenoui/app.js','zenoui/app-bootstrap.js','zenoui/app-worker.js'];
const html='<!doctype html>\n<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta http-equiv="Content-Security-Policy" content="default-src \'self\'; script-src \'self\' \'wasm-unsafe-eval\'; style-src \'self\'; worker-src \'self\'; connect-src \'self\'; object-src \'none\'; base-uri \'none\'"><title>Checked application</title><link rel="stylesheet" href="site.css"></head><body><div id="app"><p role="status">Loading the application.</p></div><script type="module" src="zenoui/app-bootstrap.js"></script></body></html>\n';
// Read every asset before creating output. Missing input cannot produce a partial app.
await Promise.all([...shared.map(f=>readFile(path.join(publicDir,f))),readFile(runtime)]);
await mkdir(path.join(out,'zenoui'),{recursive:true});
await writeFile(path.join(out,'app.json'),canonical(bundle)+'\n');
await writeFile(path.join(out,'index.html'),html);
for(const file of shared)await copyFile(path.join(publicDir,file),path.join(out,file));
await copyFile(runtime,path.join(out,'core.wasm'));
console.log(JSON.stringify({format:description.format,contract,commands:description.commands.length,domain:description.domain}));
