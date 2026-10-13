// Align everything to the leftmost object.
const ids = vc.document().layers[0].children.map((c) => c.id);
vc.run("select.set", { ids });
vc.run("object.align", { horizontal: "left", to: "selection" });
vc.document().layers[0].children.map((c) => c.bounds.x);
