// Independent decision-colour oracle; geometry is shared data, structurally checked.
function require(condition, message) {
  if (!condition) throw new Error(`grid_matches_native_result: ${message}`);
}
const same = (a,b) => a.slice(0,3).every((value,index)=>value===b[index]);
function contrast(a,b) {
  const luminance = (rgb) => rgb.slice(0,3).map(v=>v/255).map(v=>v<=0.04045?v/12.92:((v+0.055)/1.055)**2.4).reduce((total,v,i)=>total+v*[0.2126,0.7152,0.0722][i],0);
  const left=luminance(a),right=luminance(b);
  return (Math.max(left,right)+0.05)/(Math.min(left,right)+0.05);
}
export function checkGrid(snapshot,result) {
  const rows=result.grid.cells;
  require(snapshot.count===864 && rows.length===864 && snapshot.pixels.length===864,"cell count is not the domain size");
  require(snapshot.mutant===result.mutant,"grid labels belong to a stale mutation");
  require(snapshot.differences===result.differences,"differing cell count disagrees with native review");
  require(snapshot.first===result.grid.first_ordinal,"first difference is not the native first witness");
  require(snapshot.cellSize>=4,"phone cells are smaller than 4 px");
  let paintedDifferences=0;
  for (const row of rows) {
    const role=row.different ? "difference" : {Accept:"accept",Reject:"reject",CommittedFailure:"failure",Refused:"failure"}[row.class];
    require(same(snapshot.pixels[row.ordinal],snapshot.colors[role]),`cell ${row.ordinal} is coloured from another result`);
    if (same(snapshot.pixels[row.ordinal],snapshot.colors.difference)) paintedDifferences++;
  }
  // This inventory contract never commits a failure; orange cells are differences.
  require(paintedDifferences===result.differences,"painted differing-cell count is not the native count");
  if (snapshot.first!==null) require(same(snapshot.marker,snapshot.colors.ink),"native first difference is not ringed");
  for (const role of ["accept","reject","failure","difference"]) require(contrast(snapshot.colors[role],snapshot.colors.paper)>=3,`${role} cells lack non-text contrast`);
  for (const role of ["ink","muted"]) require(contrast(snapshot.colors[role],snapshot.colors.paper)>=4.5,`${role} text lacks contrast`);
  return {cells:864,differences:paintedDifferences,first:snapshot.first};
}
export function captureGrid(doc,result, { cellPosition: point, GRID }) {
  const grid=doc.querySelector(".inventory-grid"),canvas=grid.querySelector("canvas"),context=canvas.getContext("2d");
  const rgba=(x,y)=>Array.from(context.getImageData(x,y,1,1).data);
  const style=doc.defaultView.getComputedStyle(grid);
  const swatch=doc.createElement("canvas");swatch.width=1;swatch.height=1;
  const paint=swatch.getContext("2d");
  const color=(variable)=>{paint.fillStyle=style.getPropertyValue(variable).trim();paint.fillRect(0,0,1,1);return Array.from(paint.getImageData(0,0,1,1).data);};
  const colors={paper:color("--paper"),ink:color("--ink"),muted:color("--muted"),accept:color("--accept-ink"),reject:color("--reject-ink"),failure:color("--failure-ink"),difference:color("--failure-ink")};
  const first=result.grid.first_ordinal,marker=first===null?null:point(result.grid.cells[first].input);
  return {count:Number(grid.dataset.cells),mutant:grid.dataset.mutant,differences:Number(grid.dataset.differences),first:grid.dataset.firstOrdinal===""?null:Number(grid.dataset.firstOrdinal),
    colors,cellSize:GRID.size*canvas.getBoundingClientRect().width/canvas.width,
    pixels:result.grid.cells.map((row)=>{const {x,y}=point(row.input);return rgba(x+4,y+4);}),marker:marker?rgba(marker.x+4,marker.y-3):null};
}
