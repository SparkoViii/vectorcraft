class Shape {
  #name;
  static count = 0;
  constructor(name) { this.#name = name; Shape.count++; }
  get name() { return this.#name; }
  area() { return 0; }
  describe() { return `${this.name}: ${this.area().toFixed(1)}`; }
}
class Rect extends Shape {
  constructor(w, h) { super("rect"); this.w = w; this.h = h; }
  area() { return this.w * this.h; }
}
class Circle extends Shape {
  constructor(r) { super("circle"); this.r = r; }
  area() { return Math.PI * this.r ** 2; }
}
const shapes = [new Rect(3, 4), new Circle(1), new Rect(2, 2.5)];
({ count: Shape.count, descriptions: shapes.map((s) => s.describe()), isShape: shapes.every((s) => s instanceof Shape) });
