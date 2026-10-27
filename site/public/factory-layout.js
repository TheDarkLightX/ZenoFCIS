// One geometry datum for drawing, navigation, hit testing and the raster oracle.
export const GRID=Object.freeze({width:612,height:258,size:8,x:30,y:50,groupStep:148,authGap:6,step:10,quantityStep:72,stocks:6,actions:4,quantities:3});
export function cellPosition([available,reserved,action,quantity,authorized],g=GRID) {
  return {x:g.x+(action-150)*g.groupStep+(authorized*g.stocks+available)*g.step+authorized*g.authGap,
    y:g.y+(quantity-1)*g.quantityStep+reserved*g.step,column:(action-150)*g.stocks*2+authorized*g.stocks+available,row:(quantity-1)*g.stocks+reserved};
}
export function cellOrdinal(row,column,g=GRID) {
  const action=Math.floor(column/(g.stocks*2)),authorized=Math.floor(column%(g.stocks*2)/g.stocks),available=column%g.stocks;
  return (((available*g.stocks+row%g.stocks)*g.actions+action)*g.quantities+Math.floor(row/g.stocks))*2+authorized;
}
export function cellInput(ordinal,g=GRID) {
  let n=ordinal;const authorized=n%2;n=Math.floor(n/2);const quantity=n%g.quantities+1;n=Math.floor(n/g.quantities);const action=n%g.actions+150;n=Math.floor(n/g.actions);return [Math.floor(n/g.stocks),n%g.stocks,action,quantity,authorized];
}
export function hitTest(x,y,g=GRID) {
  for(let ordinal=0;ordinal<g.stocks*g.stocks*g.actions*g.quantities*2;ordinal++) {
    const p=cellPosition(cellInput(ordinal,g),g);
    if(x>=p.x&&x<p.x+g.size&&y>=p.y&&y<p.y+g.size)return ordinal;
  }
  return null;
}
export function checkLayout(g=GRID) {
  const domain=g.stocks*g.stocks*g.actions*g.quantities*2;
  if(domain!==864)throw new Error('layout_coverage: expected 864 cells');
  const boxes=[];
  for(let ordinal=0;ordinal<domain;ordinal++){
    const input=cellInput(ordinal,g),p=cellPosition(input,g);
    if(cellOrdinal(p.row,p.column,g)!==ordinal)throw new Error('layout_coverage: ordinal mismatch');
    if(p.x<0||p.y<0||p.x+g.size>g.width||p.y+g.size>g.height)throw new Error('layout_bounds');
    if(boxes.some(b=>p.x<b.x+g.size&&b.x<p.x+g.size&&p.y<b.y+g.size&&b.y<p.y+g.size))throw new Error('layout_overlap');
    for(const [dx,dy] of [[0,0],[g.size/2,g.size/2],[g.size-.01,g.size-.01]])if(hitTest(p.x+dx,p.y+dy,g)!==ordinal)throw new Error('layout_hit_inverse');
    boxes.push(p);
  }
  return domain;
}

// Width declarations are shared by the generated stylesheet and the offline
// minimum-width check. Zero-minimum tracks wrap text; only the cell canvas scrolls.
export const FACTORY_LAYOUT=Object.freeze({pageInsetRem:1.25,regions:Object.freeze([
  Object.freeze({name:'controls',selector:'.act-controls',display:'flex',columns:null,gapRem:1,insetRem:0}),
  Object.freeze({name:'instrument',selector:'.factory-instrument',columns:'minmax(0, 1fr)',gapRem:0,insetRem:0}),
  Object.freeze({name:'metrics',selector:'.factory-metrics',columns:'minmax(0, 1fr)',gapRem:0,insetRem:0}),
  Object.freeze({name:'comparison',selector:'.factory-comparison',columns:'minmax(0, 1fr)',gapRem:0,insetRem:0}),
  Object.freeze({name:'record',selector:'.technical-record',display:'block',columns:null,gapRem:0,insetRem:0}),
  Object.freeze({name:'grid',selector:'.inventory-grid',display:'block',columns:null,gapRem:0,insetRem:.75,borderPx:1}),
  Object.freeze({name:'ledger',selector:'.ledger-table',columns:'minmax(0, 1fr) minmax(0, 1.5fr)',gapRem:1.5,insetRem:0}),
  Object.freeze({name:'witness',selector:'.witness-table',columns:'minmax(0, 1fr) minmax(0, 1fr)',gapRem:1,insetRem:0}),
])});
export function factoryLayoutCSS(layout=FACTORY_LAYOUT) {
  return `/* Generated from factory-layout.js; run node site/tests/factory-width.test.mjs --write. */
main { padding-inline: ${layout.pageInsetRem}rem; }
#factory { min-width: 0; overflow-wrap: anywhere; }
${layout.regions.map(r=>`#factory ${r.selector} { display: ${r.display??'grid'}; min-width: 0; ${r.columns?`grid-template-columns: ${r.columns}; `:''}column-gap: ${r.gapRem}rem; padding-inline: ${r.insetRem}rem; ${r.borderPx?`border-width: ${r.borderPx}px;`:''} }`).join('\n')}
#factory .act-controls > *, #factory .mutation-label { min-width: 0; max-width: 100%; }
#factory .mutation-label { display: grid; grid-template-columns: minmax(0, 1fr); }
#factory .ledger-table dd { min-width: 0; padding-left: 0; overflow-wrap: anywhere; }
#factory .witness-table dd { min-width: 0; overflow-wrap: anywhere; }
#factory .inventory-grid canvas { width: ${GRID.width}px; height: ${GRID.height}px; }
`;
}
export function factoryMinimumWidths(viewport,layout=FACTORY_LAYOUT,rem=16) {
  const available=viewport-2*layout.pageInsetRem*rem;
  return layout.regions.map(region=>{
    // Deliberately accept only declared flexible tracks with numeric minima.
    // An intrinsic/max-content or fixed column needs explicit new accounting.
    const columns=region.columns??'';
    const tracks=[...columns.matchAll(/minmax\((\d+(?:\.\d+)?)(rem|px)?,\s*\d+(?:\.\d+)?fr\)/g)];
    if((columns&&!tracks.length)||columns.replace(/minmax\((\d+(?:\.\d+)?)(rem|px)?,\s*\d+(?:\.\d+)?fr\)/g,'').trim())throw new Error(`${region.name}: unaccounted minimum width`);
    const minimum=tracks.reduce((sum,t)=>sum+Number(t[1])*(t[2]==='rem'?rem:1),0)+Math.max(0,tracks.length-1)*region.gapRem*rem+2*region.insetRem*rem+2*(region.borderPx??0);
    if(minimum>available)throw new Error(`${region.name}: minimum ${minimum}px exceeds ${available}px`);
    return {region:region.name,minimum,available};
  });
}
