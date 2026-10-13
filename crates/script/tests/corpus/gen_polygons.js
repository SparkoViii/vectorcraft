// Triangle to decagon, coloured around the hue wheel.
function hslToHex(h, s, l) {
  const k = (n) => (n + h / 30) % 12;
  const a = s * Math.min(l, 1 - l);
  const f = (n) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  return "#" + [f(0), f(8), f(4)].map((v) => Math.round(v * 255).toString(16).padStart(2, "0")).join("");
}
const colours = [];
for (let sides = 3; sides <= 10; sides++) {
  const colour = hslToHex((sides - 3) * 45, 0.8, 0.5);
  const { id } = vc.run("shape.polygon", { cx: 25 + (sides - 3) * 48, cy: 150, radius: 20, sides });
  vc.run("paint.setFill", { ids: [id], color: colour });
  colours.push(colour);
}
colours;
