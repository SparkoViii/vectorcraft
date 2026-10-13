// Thirty lines radiating from the centre.
for (let i = 0; i < 30; i++) {
  const a = (i / 30) * 2 * Math.PI;
  vc.run("shape.line", { x1: 200, y1: 150, x2: 200 + 120 * Math.cos(a), y2: 150 + 120 * Math.sin(a) });
}
30;
