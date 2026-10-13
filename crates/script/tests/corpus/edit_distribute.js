// Even out the spacing of the objects' centres.
const ids = vc.document().layers[0].children.map((c) => c.id);
vc.run("select.set", { ids });
vc.run("object.distribute", { horizontal: "center" });
ids.length;
