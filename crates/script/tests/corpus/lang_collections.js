const m = new Map([["a", 1], ["b", 2]]);
m.set("c", 3);
const s = new Set([3, 1, 3, 2, 1]);
const words = ["pear", "fig", "apple", "kiwi", "date"];
const byLength = {};
for (const w of words) (byLength[w.length] ??= []).push(w);
const people = [{ n: "b", a: 2 }, { n: "a", a: 1 }, { n: "c", a: 2 }, { n: "d", a: 1 }];
const stable = [...people].sort((p, q) => p.a - q.a).map((p) => p.n).join("");
({
  mapKeys: [...m.keys()],
  mapTotal: [...m.values()].reduce((a, b) => a + b, 0),
  setSorted: [...s].sort((a, b) => a - b),
  byLength,
  pairs: Object.entries({ p: 1, q: 2 }),
  fromPairs: Object.fromEntries([["k", "v"]]),
  flat: [[1, [2]], [3]].flat(2),
  flatMap: [1, 2].flatMap((n) => [n, n * 10]),
  stable,
  includes: words.includes("fig"),
});
