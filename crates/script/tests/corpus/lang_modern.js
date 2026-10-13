const cfg = { a: { b: null } };
let count = 0;
count ||= 5;
count &&= count + 1;
let missing = null;
missing ??= "default";
const big = 2n ** 64n;
const arr = [10, 20, 30];
const label = (n) => { switch (true) { case n < 15: return "low"; case n < 25: return "mid"; default: return "high"; } };
({
  optional: cfg.a?.b?.c ?? "none",
  call: cfg.missing?.() ?? "no call",
  count,
  missing,
  big: big.toString(),
  at: arr.at(-1),
  labels: arr.map(label),
  exp: 2 ** 10,
  numericSep: 1_000_000,
  objectSpread: { ...{ a: 1 }, b: 2 },
  hasOwn: Object.hasOwn({ q: 1 }, "q"),
  findLast: arr.findLast((n) => n < 25),
});
