// Draw, then export to the path given as --arg out=….
const { id } = vc.run("shape.ellipse", { x: 50, y: 50, width: 200, height: 120 });
vc.run("paint.setFill", { ids: [id], color: "#ff6600" });
vc.run("document.export", { path: vc.args.out });
vc.args.out;
