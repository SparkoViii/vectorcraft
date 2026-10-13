// Unbounded recursion: the recursion limit stops it.
vc.run("shape.rectangle", { x: 10, y: 10, width: 20, height: 20 });
function down(n) { return down(n + 1) + 1; }
down(0);
