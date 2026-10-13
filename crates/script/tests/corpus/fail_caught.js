// A failing command, caught: the script goes on.
let message = "";
try {
  vc.run("no.such.command", {});
} catch (e) {
  message = e.message;
}
vc.run("shape.rectangle", { x: 10, y: 10, width: 20, height: 20 });
message;
