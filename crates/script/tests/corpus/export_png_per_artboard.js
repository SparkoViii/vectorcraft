// Two artboards, each exported as a PNG into --arg dir=….
vc.run("shape.rectangle", { x: 20, y: 20, width: 100, height: 100 });
vc.run("artboard.new", { x: 450, y: 0, width: 200, height: 200, name: "Second" });
vc.run("shape.ellipse", { x: 480, y: 30, width: 100, height: 100 });
const boards = vc.document().artboards.length;
for (let i = 0; i < boards; i++) {
  vc.run("document.export", { path: `${vc.args.dir}/board-${i}.png`, artboard: i });
}
boards;
