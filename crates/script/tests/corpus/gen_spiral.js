// Circles along an Archimedean spiral, growing outwards.
const cx = 200, cy = 150;
const ids = [];
for (let i = 0; i < 36; i++) {
  const angle = i * 0.5;
  const r = 4 * angle;
  const size = 4 + i * 0.4;
  const { id } = vc.run("shape.ellipse", {
    x: cx + r * Math.cos(angle) - size / 2,
    y: cy + r * Math.sin(angle) - size / 2,
    width: size,
    height: size,
  });
  ids.push(id);
}
new Set(ids).size;
