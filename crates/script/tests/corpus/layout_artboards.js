// An artboard per icon size, in a row right of the first.
const sizes = [16, 32, 64, 128];
let x = 450;
for (const size of sizes) {
  vc.run("artboard.new", { x, y: 0, width: size, height: size, name: `Icon ${size}` });
  x += size + 20;
}
vc.document().artboards.length;
