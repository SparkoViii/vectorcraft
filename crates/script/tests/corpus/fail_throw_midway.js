// Draws three squares, then fails: nothing is left behind.
for (let i = 0; i < 3; i++) vc.run("shape.rectangle", { x: 20 + i * 40, y: 20, width: 30, height: 30 });
vc.log("drew 3");
throw new Error("stop here");
