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
