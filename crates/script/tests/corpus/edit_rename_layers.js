// Number the layers from the top: Part 1, Part 2, …
const layers = vc.document().layers;
layers.forEach((layer, i) => vc.run("layer.setProps", { id: layer.id, name: `Part ${i + 1}` }));
vc.document().layers.map((l) => l.name);
