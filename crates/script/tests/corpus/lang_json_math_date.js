const data = { a: [1, 2, { b: null }], c: "é", d: 1.5 };
const round = JSON.parse(JSON.stringify(data));
({
  roundTrip: JSON.stringify(round) === JSON.stringify(data),
  pretty: JSON.stringify({ k: [1] }, null, 1),
  hypot: Math.hypot(3, 4),
  sin: Math.round(Math.sin(Math.PI / 2) * 1000) / 1000,
  epsilonSmall: Number.EPSILON < 1e-15,
  maxSafe: Number.MAX_SAFE_INTEGER,
  utc: new Date(Date.UTC(2026, 9, 11, 12, 30)).toISOString(),
  dayOfWeek: new Date(Date.UTC(2026, 9, 11)).getUTCDay(),
  parsed: Date.parse("2026-10-11T00:00:00Z"),
  fixed: (3.14159).toFixed(2),
});
