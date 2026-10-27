// Presentation only: identifiers come from the template's parsed declarations.
export const human = (name) => String(name).replaceAll("_", " ").replace(/([a-z])([A-Z])/g,"$1 $2");
export const yesNo = (value) => value === 1 || value === true ? "yes" : "no";
export function inventoryInput(input, names) {
  const labels = names.inputs.map((field) => field.name);
  return `${labels[0]} ${input[0]} · ${labels[1]} ${input[1]} · ${labels[2]} ${names.variants[input[2]]} · ${labels[3]} ${input[3]} · operator ${labels[4]} ${yesNo(input[4])}`;
}
export function inventoryDecision(decision, names) {
  const after = decision.class === "Reject" ? "After: unchanged."
    : `After: ${(decision.post ?? []).map((field) => `${names.fields[field.field]} ${field.value}`).join(" · ") || "no successor returned"}.`;
  return {
    label: { Accept: "Accepted", Reject: "Rejected", CommittedFailure: "Committed failure", Refused: "Refused before a decision" }[decision.class],
    reason: decision.reason ? human(names.reasons[decision.reason]) : decision.refusal ? "The library refused this result." : "No refusal reason.",
    after,
    effects: (decision.outbox ?? []).map((entry) => `${names.channels[entry.channel]} to ${entry.destination}: ${entry.payload.map((field) => `${names.fields[field.field]} ${field.value}`).join(" · ")}`),
    laws: decision.laws.filter((law) => law.status !== "Skipped").map((law) => `${human(names.laws[law.id])}: ${law.status === "Satisfied" ? "satisfied" : "refused"}`),
  };
}
export function synthesisInput(input, result) {
  const facts = result.inputs.slice(1).filter((_, index) => input[index + 1] === 1).map((name) => `${human(name.replace(/^fact\./,""))} yes`);
  return [`action ${result.names.sums.TreasuryAction[input[0]]}`, ...facts].join(" · ");
}
export function synthesisOutput(output, names) {
  if (output[0] === 0) return "Accepted";
  return `${output[0] === 13 ? "Committed failure" : "Rejected"}: ${human(names.reason_ranks[output[0]])}`;
}
export const booleanTuple = (values, label) => values.map((value, index) => `${label} ${index + 1}: ${yesNo(value)}`).join(" · ");
export const amount = (value) => value === "340282366920938463463374607431768211455" ? "2^128 − 1 (the largest 128-bit value)" : value;
