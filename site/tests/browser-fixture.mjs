// A small DOM/worker adapter for executing the shipped page module in Node.
// This checks events and data flow; it supplies no layout or raster evidence.
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
export const ORIGIN='https://zenoui.test';
const PUBLIC=new URL('../public/',import.meta.url),HARNESS=new URL('./harness/',import.meta.url);
export function assetFile(url) {
  const parsed=new URL(url);
  if(parsed.origin!==ORIGIN)throw new Error(`External test request: ${url}`);
  for(const [prefix,directory] of [['/ZenoFCIS/',PUBLIC],['/harness/',HARNESS]]) {
    if(parsed.pathname.startsWith(prefix))return new URL(parsed.pathname.slice(prefix.length),directory);
  }
  throw new Error(`Unserved asset: ${parsed.pathname}`);
}
export class Element extends EventTarget {
  constructor(tag='div'){super();this.tagName=tag.toUpperCase();this.children=[];this.dataset={};this.style={};this.attributes={};this.className='';}
  append(...items){for(const item of items){const child=typeof item==='object'?item:new Text(String(item));if(child.parent)child.parent.children=child.parent.children.filter(c=>c!==child);child.parent=this;this.children.push(child);}}
  replaceChildren(...items){this.children=[];this.append(...items);}
  get textContent(){return this.children.map(c=>c.textContent).join('');}
  set textContent(text){this.replaceChildren(String(text));}
  get firstChild(){return this.children[0];}
  setAttribute(key,value){this.attributes[key]=String(value);if(key==='class')this.className=String(value);if(key==='id')this.id=String(value);if(key==='value')this.value=String(value);if(key.startsWith('data-'))this.dataset[key.slice(5).replace(/-([a-z])/g,(_,c)=>c.toUpperCase())]=String(value);}
  removeAttribute(key){delete this.attributes[key];}
  focus(){let root=this;while(root.parent)root=root.parent;root.activeElement=this;}
  getAttribute(key){return this.attributes[key]??null;}
  matches(selector){
    const compound=selector.replace(/\[[^\]]*\]/g,'');
    const tag=compound.match(/^[a-z]+/i)?.[0];if(tag&&this.tagName!==tag.toUpperCase())return false;
    const id=compound.match(/#([\w-]+)/)?.[1];if(id&&this.id!==id)return false;
    for(const [,name] of compound.matchAll(/\.([\w-]+)/g))if(!this.className.split(/\s+/).includes(name))return false;
    for(const [,key,value] of selector.matchAll(/\[([\w-]+)(?:="([^"]*)")?\]/g)){
      const actual=key.startsWith('data-')?this.dataset[key.slice(5).replace(/-([a-z])/g,(_,c)=>c.toUpperCase())]:this.getAttribute(key);
      if(actual===null||actual===undefined||(value!==undefined&&actual!==value))return false;
    }
    return true;
  }
  querySelectorAll(selector){
    const parts=selector.split(/\s+/),last=parts.at(-1),found=[];
    const visit=node=>{for(const child of node.children){if(child instanceof Text)continue;if(child.matches(last)){
      let parent=child.parent,index=parts.length-2;
      while(parent&&index>=0){if(parent.matches(parts[index]))index--;parent=parent.parent;}
      if(index<0)found.push(child);
    }visit(child);}};visit(this);return found;
  }
  querySelector(selector){return this.querySelectorAll(selector)[0]??null;}
  click(){if(!this.disabled)this.dispatchEvent(new Event('click'));}
}
class Text {constructor(text){this.textContent=text;}}
export function documentFrom(html){
  const doc=new Element('document'),stack=[doc];
  for(const token of html.matchAll(/<!--[\s\S]*?-->|<![^>]*>|<\/?[a-z][^>]*>|[^<]+/gi)){
    const raw=token[0];if(raw.startsWith('<!'))continue;
    if(raw.startsWith('</')){if(stack.length>1)stack.pop();continue;}
    if(!raw.startsWith('<')){stack.at(-1).append(raw);continue;}
    const tag=raw.match(/^<([a-z][\w-]*)/i)[1],node=new Element(tag);
    for(const [,key,a,b] of raw.slice(tag.length+1,-1).matchAll(/([\w-]+)(?:="([^"]*)"|='([^']*)')?/g))node.setAttribute(key,a??b??'');
    stack.at(-1).append(node);
    if(!['area','base','br','col','embed','hr','img','input','link','meta','param','source','track','wbr'].includes(tag))stack.push(node);
  }
  doc.createElement=tag=>new Element(tag);doc.createTextNode=text=>new Text(String(text));doc.getElementById=id=>doc.querySelector(`#${id}`);return doc;
}
export function moduleLoader(globals, transform=(_url,source)=>source, resolveAsset=assetFile){
  const context=vm.createContext({URL,URLSearchParams,console,structuredClone,performance,setTimeout,clearTimeout,queueMicrotask,Event,CustomEvent,WebAssembly,TextEncoder,TextDecoder,...globals}),modules=new Map(),evaluations=new Map();
  // Cache construction immediately: multiple imports of a shared dependency
  // must hand the same module to the VM's graph linker.
  function construct(url){
    if(!modules.has(url))modules.set(url,(async()=>{
      const source=await readFile(resolveAsset(url),'utf8');
      return new vm.SourceTextModule(transform(url,source),{context,identifier:url,initializeImportMeta(meta){meta.url=url;},
        importModuleDynamically:async specifier=>evaluate(new URL(specifier,url).href)});
    })());
    return modules.get(url);
  }
  // Serialize independent root links; the VM recursively links dependencies.
  let links=Promise.resolve();
  function load(url){
    const result=links.then(async()=>{
      const module=await construct(url);
      if(module.status==='unlinked')await module.link((specifier,parent)=>construct(new URL(specifier,parent.identifier).href));
      if(module.status==='errored')throw module.error;
      return module;
    });
    links=result.then(()=>{},()=>{});
    return result;
  }
  function evaluate(url){
    if(!evaluations.has(url))evaluations.set(url,(async()=>{const module=await load(url);if(module.status==='linked')await module.evaluate();if(module.status==='errored')throw module.error;return module;})());
    return evaluations.get(url);
  }
  return {context,load,evaluate};
}
export function localWorkerClass(resolveAsset=assetFile){
  return class LocalWorker {
    constructor(url,options){
      if(options.type!=='module')throw new Error('Module worker required');
      const self={postMessage:data=>queueMicrotask(()=>this.onmessage?.({data:structuredClone(data)}))};
      const loader=moduleLoader({self,fetch:async url=>{
        const bytes=await readFile(resolveAsset(url));return {ok:true,arrayBuffer:async()=>bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength)};
      }},undefined,resolveAsset);
      this.ready=loader.evaluate(url.href).then(()=>self);this.ready.catch(()=>{});
    }
    postMessage(data){this.ready.then(self=>self.onmessage({data:structuredClone(data)})).catch(error=>this.onerror?.(error));}
    terminate(){this.onmessage=null;this.onerror=null;}
  };
}
