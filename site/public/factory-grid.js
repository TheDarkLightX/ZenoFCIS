import { inventoryInput } from "./factory-format.js";

import { GRID, cellPosition, cellOrdinal, hitTest } from "./factory-layout.js";
export { GRID, cellPosition, cellOrdinal } from "./factory-layout.js";
export const cellRole = (cell) => cell.different ? "difference" : ({ Accept: "accept", Reject: "reject", CommittedFailure: "failure", Refused: "failure" })[cell.class];
export function paintInventory(context, cells, names, colors, first, selected) {
  context.fillStyle = colors.paper;
  context.fillRect(0, 0, GRID.width, GRID.height);
  context.textBaseline = "alphabetic";
  for (let action = 0; action < 4; action++) {
    const x = GRID.x + action * GRID.groupStep;
    context.fillStyle = colors.ink;
    context.font = "600 12px system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif";
    context.fillText(names?.variants[150 + action] ?? `Action ${action + 1}`, x, 16);
    context.fillStyle = colors.muted;
    context.font = "10px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";
    context.fillText("no", x, 38);
    context.fillText("yes", x + GRID.stocks * GRID.step + GRID.authGap, 38);
  }
  context.fillStyle = colors.muted;
  context.font = "10px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";
  context.fillText("qty", 2, 38);
  for (let quantity = 0; quantity < 3; quantity++) context.fillText(String(quantity + 1), 8, GRID.y + quantity * GRID.quantityStep + 33);
  // Unchecked cells are outlines; none receives a decision colour in advance.
  context.strokeStyle = colors.line;
  context.lineWidth = 1;
  for (let row = 0; row < 18; row++) for (let column = 0; column < 48; column++) {
    const available = column % 6, authorized = Math.floor((column % 12) / 6);
    const { x,y } = cellPosition([available,row % 6,150 + Math.floor(column / 12),1 + Math.floor(row / 6),authorized]);
    context.strokeRect(x + 0.5,y + 0.5,GRID.size - 1,GRID.size - 1);
  }
  for (const cell of cells) {
    if (!cell) continue;
    const { x,y } = cellPosition(cell.input);
    const role = cellRole(cell);
    if (!role) throw new Error("Unknown inventory decision class");
    context.fillStyle = colors[role];
    context.fillRect(x,y,GRID.size,GRID.size);
  }
  if (first !== null && cells[first]) {
    const {x,y} = cellPosition(cells[first].input);
    context.beginPath(); context.arc(x + 4,y + 4,7,0,Math.PI * 2);
    context.strokeStyle = colors.ink; context.lineWidth = 2; context.stroke();
  }
  if (selected !== null && cells[selected]) {
    const {x,y} = cellPosition(cells[selected].input);
    context.setLineDash([2,2]); context.strokeStyle = colors.ink; context.lineWidth = 1;
    context.strokeRect(x - 3.5,y - 3.5,15,15); context.setLineDash([]);
  }
}
function node(tag, className, text) {
  const result = document.createElement(tag);
  if (className) result.className = className;
  if (text) result.textContent = text;
  return result;
}
export function mountInventoryGrid(container, mutant, job, onSelect) {
  const wrapper = node("div","inventory-grid");
  wrapper.tabIndex = -1;
  wrapper.setAttribute("role","grid");
  wrapper.setAttribute("aria-rowcount","18"); wrapper.setAttribute("aria-colcount","48");
  wrapper.setAttribute("aria-label","864 inventory inputs. Use arrow keys to select a cell; Home and End move across a row.");
  wrapper.setAttribute("aria-busy","true");
  wrapper.dataset.mutant = mutant; wrapper.dataset.job = job;
  const canvas = node("canvas"); canvas.width = GRID.width; canvas.height = GRID.height;
  canvas.setAttribute("aria-hidden","true");
  const activeRow = node("div","visually-hidden"); activeRow.setAttribute("role","row");
  const activeCell = node("span"); activeCell.setAttribute("role","gridcell"); activeCell.id = `inventory-cell-${job}`;
  activeRow.append(activeCell); wrapper.setAttribute("aria-activedescendant",activeCell.id);
  wrapper.append(canvas,activeRow);
  const count = node("p","grid-count","0 of 864 inputs decided");
  const help = node("p","grid-help","Each square is one input. In every block, available stock runs 0 to 5 left to right and reserved stock 0 to 5 top to bottom; the left block of each action is without operator authorization, the right with it. Click a square, or use the arrow keys, to see its decision.");
  const legend = node("ul","grid-legend");
  for (const [role,label] of [["accept","Accepted"],["reject","Rejected"],["difference","Changed by the bug"]]) {
    const item = node("li"); item.append(node("span",`grid-key ${role}`),document.createTextNode(label)); legend.append(item);
  }
  const selection = node("p","grid-selection"); selection.setAttribute("role","status");
  container.append(count,wrapper,legend,help,selection);
  let cells = Array(864).fill(null), names, first = null, selected = null, enabled = false;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  const theme = matchMedia("(prefers-color-scheme: dark)");
  function paint() {
    const style = getComputedStyle(wrapper);
    const color = (name) => style.getPropertyValue(name).trim();
    const colors = { paper:color("--paper"),ink:color("--ink"),muted:color("--muted"),line:color("--line"),
      accept:color("--accept-ink"),reject:color("--reject-ink"),failure:color("--failure-ink"),difference:color("--failure-ink") };
    paintInventory(context,cells,names,colors,first,selected);
  }
  theme.addEventListener("change",paint);
  function select(ordinal) {
    if (!enabled || !cells[ordinal]) return;
    selected = ordinal;
    const cell = cells[ordinal], position = cellPosition(cell.input);
    const label = inventoryInput(cell.input,names);
    selection.textContent = label;
    activeCell.textContent = label; activeCell.setAttribute("aria-colindex",position.column + 1);
    activeRow.setAttribute("aria-rowindex",position.row + 1);
    wrapper.dataset.selectedOrdinal = ordinal;
    wrapper.scrollLeft = Math.max(0,Math.min(wrapper.scrollLeft,position.x - 6));
    if (position.x + 16 > wrapper.scrollLeft + wrapper.clientWidth) wrapper.scrollLeft = position.x + 16 - wrapper.clientWidth;
    paint(); onSelect(cell,names);
  }
  wrapper.addEventListener("click", (event) => {
    if (event.target !== canvas || !enabled) return;
    const rect = canvas.getBoundingClientRect(), x = (event.clientX - rect.left) * canvas.width / rect.width, y = (event.clientY - rect.top) * canvas.height / rect.height;
    // Hit testing uses the same declared rectangles as drawing.
    const ordinal = hitTest(x,y);
    const cell = ordinal === null ? null : cells[ordinal];
    if (cell) { wrapper.focus({preventScroll:true}); select(cell.ordinal); }
  });
  wrapper.addEventListener("keydown", (event) => {
    if (!enabled || !["ArrowLeft","ArrowRight","ArrowUp","ArrowDown","Home","End"].includes(event.key)) return;
    event.preventDefault();
    let {row,column} = cellPosition(cells[selected ?? 0].input);
    if (event.key === "ArrowLeft") column--;
    if (event.key === "ArrowRight") column++;
    if (event.key === "ArrowUp") row--;
    if (event.key === "ArrowDown") row++;
    if (event.key === "Home") { column = 0; if (event.ctrlKey) row = 0; }
    if (event.key === "End") { column = 47; if (event.ctrlKey) row = 17; }
    select(cellOrdinal(Math.max(0,Math.min(17,row)),Math.max(0,Math.min(47,column))));
  });
  paint();
  return {
    append(chunk) {
      if (chunk.mutant !== mutant) throw new Error("Grid rows belong to another mutation");
      names = chunk.names;
      for (const cell of chunk.cells) cells[cell.ordinal] = cell;
      wrapper.dataset.cells = cells.filter(Boolean).length;
      count.textContent = `${wrapper.dataset.cells} of 864 inputs decided`;
      paint();
    },
    finish(result) {
      if (result.grid.mutant !== mutant || result.grid.cells.length !== 864) throw new Error("Grid result belongs to another run");
      cells = result.grid.cells; names = result.names; first = result.grid.first_ordinal;
      wrapper.dataset.cells = cells.length; wrapper.dataset.differences = cells.filter((cell) => cell.different).length;
      wrapper.dataset.firstOrdinal = first ?? "";
      wrapper.setAttribute("aria-busy","false");
      count.textContent = mutant === "none" ? "864 of 864 inputs decided" : result.differences === 0
        ? "No decision changed on any of the 864 inputs" : `${result.differences} of 864 decisions changed`;
      enabled = true; wrapper.tabIndex = 0; select(first ?? 0);
    },
    setEnabled(value) { enabled = value; wrapper.tabIndex = value ? 0 : -1; wrapper.setAttribute("aria-disabled",String(!value)); },
    dispose() { theme.removeEventListener("change",paint); },
  };
}
