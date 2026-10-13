// A 3 × 3 sheet: a frame and a caption per cell.
let made = 0;
for (let i = 0; i < 9; i++) {
  const x = 20 + (i % 3) * 125, y = 15 + Math.floor(i / 3) * 95;
  vc.run("shape.rectangle", { x, y, width: 110, height: 70 });
  vc.run("text.create", { x, y: y + 85, text: `Frame ${i + 1}`, size: 9 });
  made += 2;
}
made;
