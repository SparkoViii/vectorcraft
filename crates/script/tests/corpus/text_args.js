// A title from the arguments (vectorcraft-cli script … --arg title=…).
const title = vc.args.title ?? "Untitled";
vc.run("text.create", { x: 20, y: 60, text: title, size: 32 });
title;
