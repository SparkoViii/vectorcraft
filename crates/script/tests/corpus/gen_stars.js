// Twelve stars with 5 to 16 points.
let points = 0;
for (let i = 0; i < 12; i++) {
  const n = 5 + i;
  vc.run("shape.star", { cx: 30 + (i % 6) * 60, cy: 60 + Math.floor(i / 6) * 100, radius1: 25, radius2: 12, points: n });
  points += n;
}
points;
