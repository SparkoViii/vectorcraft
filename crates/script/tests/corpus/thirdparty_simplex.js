// Uses simplex-noise 2.4.0 (MIT) as `SimplexNoise`: a noise field of bars.
const noise = new SimplexNoise(() => 0.5);
let made = 0;
for (let i = 0; i < 20; i++) {
  const h = 20 + 100 * (noise.noise2D(i / 8, 0.3) + 1) / 2;
  vc.run("shape.rectangle", { x: 10 + i * 19, y: 280 - h, width: 15, height: h });
  made++;
}
made;
