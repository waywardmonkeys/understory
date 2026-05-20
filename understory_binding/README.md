# understory_binding

`understory_binding` provides small one-way property binding primitives for
Understory.

This crate owns binding declarations, dependency ordering, dirty binding
selection, and deterministic binding evaluation. It explicitly does not own
property storage, style resolution, opinion composition, widget trees, host
scheduling, or application invalidation policy.

Hosts expose erased property endpoint reads and writes through `BindingHost`.
`BindingSet` tracks dirty bindings with an internal `invalidation` tracker,
evaluates them in dependency order, and returns the application channels affected
by binding target writes.
