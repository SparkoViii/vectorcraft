// A 5 × 4 grid of squares.
let made = 0;
for (let row = 0; row < 4; row++) {
  for (let col = 0; col < 5; col++) {
    vc.run("shape.rectangle", { x: 20 + col * 40, y: 20 + row * 40, width: 30, height: 30 });
    made++;
  }
}
made;
