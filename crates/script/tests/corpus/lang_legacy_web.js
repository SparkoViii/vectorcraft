// The legacy web features every browser has (Annex B), which real scripts still use.
({
  substr: "abcdef".substr(-2),
  escape: escape("a b"),
  unescape: unescape("%41"),
  bold: "x".bold(),
  proto: ({ __proto__: { inherited: true } }).inherited,
  trimLeft: "  x".trimLeft(),
});
