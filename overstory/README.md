# Overstory

Overstory is a semantic UI runtime built on Understory primitives.

It owns control meaning, style/property resolution, template expansion,
measure/arrange layout, and visual emission into `imaging::record::Scene`. It
does not own platform event loops, renderer backends, or Understory's structural
and input substrate.

The first slice proves a small retained pipeline:

```text
semantic control -> style/property resolution -> template -> measure -> arrange -> imaging scene
```

## Widget Boundary

`Ui` owns element identity, parent/child structure, dependency properties,
style resolution, invalidation, and phase scheduling. Widgets own kind-specific
measurement and presentation emission.

The built-in widget set starts with `Button`, `TextBlock`, `Panel`, `Row`, and
`Toggle`, but the core dispatch is open: applications can append any `Widget`
implementation with an application-defined `ElementKind`.

This replaces the earlier closed `ElementKind::Button` path. Text content now
lives in text-bearing widgets instead of a global `Content` dependency property;
templates receive content through `TemplateValues`.

`Panel` and `Row` own their child layout policies. `Ui` provides scheduling,
property resolution, text services, and child measurement/presentation helpers;
it does not switch on widget kinds to arrange children.

Pointer hit testing and activation also route through widgets. `Toggle` uses
that path to mutate its own checked state and invalidate the retained
presentation.
