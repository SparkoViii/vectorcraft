// Name objects left to right: Tile 1, Tile 2, …
const objects = vc.document().layers[0].children.slice().sort((a, b) => a.bounds.x - b.bounds.x);
objects.forEach((o, i) => vc.run("object.setProps", { id: o.id, name: `Tile ${i + 1}` }));
objects.length;
