// Three rows of four circles, each row a group.
for (let row = 0; row < 3; row++) {
  const ids = [];
  for (let col = 0; col < 4; col++) {
    ids.push(vc.run("shape.ellipse", { x: 30 + col * 50, y: 30 + row * 60, width: 40, height: 40 }).id);
  }
  vc.run("select.set", { ids });
  vc.run("object.group", {});
}
vc.document().layers[0].children.length;
