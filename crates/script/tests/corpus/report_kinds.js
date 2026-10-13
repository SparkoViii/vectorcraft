// Count objects by kind, changing nothing.
const counts = {};
function walk(nodes) {
  for (const n of nodes) {
    counts[n.kind] = (counts[n.kind] ?? 0) + 1;
    if (n.children) walk(n.children);
  }
}
walk(vc.document().layers[0].children);
counts;
