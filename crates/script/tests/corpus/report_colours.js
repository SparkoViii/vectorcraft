// The distinct fills in use.
[...new Set(vc.document().layers[0].children.map((c) => c.fill))].sort();
