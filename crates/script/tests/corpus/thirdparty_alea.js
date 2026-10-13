// Uses Alea (seedrandom 3.0.5, MIT) as `alea`: a seeded scatter that repeats exactly.
const rng = alea("vectorcraft");
const points = [];
for (let i = 0; i < 40; i++) {
  const x = rng() * 380, y = rng() * 280;
  vc.run("shape.ellipse", { x, y, width: 8, height: 8 });
  points.push(Math.round(x * 100) / 100);
}
points;
