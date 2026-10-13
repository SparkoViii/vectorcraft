const m = "2026-10-11".match(/(?<y>\d{4})-(?<m>\d{2})-(?<d>\d{2})/);
const name = "VectorCraft";
({
  year: Number(m.groups.y),
  month: m.groups.m,
  replaced: "a-b-c".replaceAll("-", "+"),
  template: `${name} has ${name.length} letters`,
  padded: "7".padStart(3, "0"),
  emojiLength: "\u{1F600}".length,
  codePoints: [..."a\u{1F600}b"].length,
  raw: String.raw`\n`,
  upper: "straße".toUpperCase(),
  sticky: /o/y.test("foo"),
  split: "one, two,three".split(/,\s*/),
  trimmed: "  x  ".trim(),
});
