// Eight squares, each turned 15° more than the last.
for (let i = 0; i < 8; i++) {
  const { id } = vc.run("shape.rectangle", { x: 20 + i * 45, y: 130, width: 30, height: 30 });
  vc.run("select.set", { ids: [id] });
  vc.run("object.rotate", { angle: i * 15 });
}
8;
