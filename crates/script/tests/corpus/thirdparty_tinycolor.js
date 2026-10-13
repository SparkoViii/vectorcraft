// Uses tinycolor 1.4.2 (MIT, tests/thirdparty/tinycolor.js) as `tinycolor`: a palette from one colour.
const base = tinycolor("#3366ff");
const palette = [base, base.clone().lighten(20), base.clone().darken(20), base.complement(), base.clone().spin(90)];
palette.forEach((c, i) => {
  const { id } = vc.run("shape.rectangle", { x: 10 + i * 60, y: 10, width: 50, height: 50 });
  vc.run("paint.setFill", { ids: [id], color: c.toHexString() });
});
palette.map((c) => c.toHexString());
