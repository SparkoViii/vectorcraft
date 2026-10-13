// One line of type per record.
const people = [
  { name: "Ada", age: 36 },
  { name: "Grace", age: 85 },
  { name: "Linus", age: 54 },
];
const lines = people.map(({ name, age }) => `${name} — ${age}`);
lines.forEach((text, i) => vc.run("text.create", { x: 20, y: 40 + i * 30, text, size: 14 }));
lines;
