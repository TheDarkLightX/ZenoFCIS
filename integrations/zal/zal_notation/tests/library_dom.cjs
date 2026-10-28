// Synthetic DOM event/focus adapter, reduced from the pinned framework browser fixture.
// No layout, raster, native keyboard-default or browser assurance.
class Element extends EventTarget {
  constructor(tag='div'){super();this.tagName=tag.toUpperCase();this.children=[];this.dataset={};this.style={};this.attributes={};this.className='';this._value=undefined;}
  get value(){if(this._value!==undefined)return this._value;if(this.tagName==='SELECT')return this.children[0]?.value??'';return '';}
  set value(value){this._value=String(value);}
  prepend(...items){const old=this.children;this.children=[];this.append(...items,...old);}
  append(...items){for(const item of items){const child=typeof item==='object'?item:new Text(String(item));if(child.parent)child.parent.children=child.parent.children.filter(c=>c!==child);child.parent=this;this.children.push(child);}}
  replaceChildren(...items){let root=this;while(root.parent)root=root.parent;const contains=n=>n===root.activeElement||n.children?.some(contains);if(this.children.some(contains))root.activeElement=root.querySelector?.('body')??root;for(const c of this.children)c.parent=null;this.children=[];this.append(...items);}
  get textContent(){return this.children.map(c=>c.textContent).join('');}
  set textContent(text){this.replaceChildren(String(text));}
  get firstChild(){return this.children[0];}
  setAttribute(key,value){this.attributes[key]=String(value);if(key==='class')this.className=String(value);if(key==='id')this.id=String(value);if(key==='value')this.value=String(value);if(key.startsWith('data-'))this.dataset[key.slice(5).replace(/-([a-z])/g,(_,c)=>c.toUpperCase())]=String(value);}
  removeAttribute(key){delete this.attributes[key];}
  focus(options){let root=this;while(root.parent)root=root.parent;root.activeElement=this;this.lastFocusOptions=options;}
  scrollIntoView(options){this.scrollCalls=(this.scrollCalls??0)+1;this.lastScrollOptions=options;}
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
function documentFrom(html){
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

module.exports={documentFrom};
