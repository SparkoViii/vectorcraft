// Uses bezier-easing 2.1.0 (MIT) as `BezierEasing`: circles spaced along an ease-in-out curve.
const ease = BezierEasing(0.42, 0, 0.58, 1);
const xs = [];
for (let i = 0; i <= 10; i++) {
  const x = 20 + 360 * ease(i / 10);
  vc.run("shape.ellipse", { x, y: 140, width: 16, height: 16 });
  xs.push(Math.round(x));
}
xs;
