// The worker and offline replay share this request sequence. No expected result
// is imported into the runtime: the library computes every displayed value.
export function runFactory(api, act, progress = () => {}, options = {}) {
  api.reset();
  const step = (request) => {
    const reply = api.step(request);
    if (reply.error) throw new Error(reply.error);
    return reply;
  };
  if (act === "synthesis") {
    const candidates = [];
    for (let number = 0; number < 8; number++) {
      const candidate = step({ act: "candidate", number });
      candidates.push(candidate);
      progress(candidate);
      if (candidate.status === "selected") break;
    }
    return { ...step({ act }), candidates };
  }
  if (act.startsWith("inventory/")) {
    const mutant = act.split("/")[1];
    const cells = [];
    for (let start = 0; start < 864; start += 48) {
      const chunk = step({ act: "inventory_rows", mutant, start, count: 48 });
      if (chunk.mutant !== mutant || chunk.start !== start || chunk.cells.length !== 48
          || chunk.cells.some((cell, index) => cell.ordinal !== start + index)) {
        throw new Error("Incomplete or mismatched inventory rows");
      }
      cells.push(...chunk.cells);
      progress({ kind: "inventory", ...chunk });
    }
    const result = step({ act: "inventory", mutant });
    const first_ordinal = result.first_difference ? cells.findIndex((cell) => JSON.stringify(cell.input) === JSON.stringify(result.first_difference.input)) : null;
    if (cells.filter((cell) => cell.different).length !== result.differences || first_ordinal === -1) {
      throw new Error("Inventory rows disagree with the full review");
    }
    return { ...result, grid: { mutant, cells, first_ordinal } };
  }
  return step({ ...options, act });
}

// A cell inspection is a new bounded, stateless invocation, never a cached decision.
export function inspectInventory(api, mutant, ordinal) {
  api.reset();
  const result = api.step({ act: "inventory_detail", mutant, ordinal });
  if (result.error) throw new Error(result.error);
  return result;
}
