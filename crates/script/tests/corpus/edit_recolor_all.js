// Recolour every object on the top layer.
const ids = vc.document().layers[0].children.map((c) => c.id);
vc.run("paint.setFill", { ids, color: "#00aa55" });
ids.length;
