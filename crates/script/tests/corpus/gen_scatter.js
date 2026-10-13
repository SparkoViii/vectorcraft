// Seeded random scatter: the same seed draws the same picture.
function lcg(seed) {
  let s = seed >>> 0;
  return () => (s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 4294967296;
}
const rand = lcg(42);
const placed = [];
for (let i = 0; i < 50; i++) {
  const d = 5 + rand() * 15;
  const x = rand() * (400 - d);
  const y = rand() * (300 - d);
  vc.run("shape.ellipse", { x, y, width: d, height: d });
  placed.push([Math.round(x), Math.round(y)]);
}
placed;
