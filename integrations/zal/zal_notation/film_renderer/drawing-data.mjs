// Coordinate lookup for a checked event history. Pixels belong to the painter.
export function points(walk, route) {
  return Array.from({ length: route.length + 1 }, (_, i) => walk.coordinates[walk.states[route.slice(0, i)]]);
}
