function counter(start) { let n = start; return () => n++; }
const next = counter(5);
next(); next();
function* fib() { let [a, b] = [0, 1]; for (;;) { yield a; [a, b] = [b, a + b]; } }
const firstTen = [];
for (const f of fib()) { if (firstTen.length === 10) break; firstTen.push(f); }
const { x = 1, y = 2, ...rest } = { y: 20, z: 30, w: 40 };
const sum = (...xs) => xs.reduce((a, b) => a + b, 0);
({ counter: next(), firstTen, x, y, rest, sum: sum(...firstTen) });
