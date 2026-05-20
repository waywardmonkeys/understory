// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![no_std]

//! Understory Binding: small one-way property binding primitives.
//!
//! This crate owns binding declarations, binding dependency ordering, dirty
//! binding selection, and deterministic binding evaluation. It explicitly does
//! not own property storage, style resolution, opinion composition, widget
//! trees, host scheduling, or application invalidation policy.
//!
//! The intended first use is control-template style glue: one property endpoint
//! feeds another property endpoint, and the host decides how those endpoints map
//! onto its retained objects.
//!
//! ## Invalidation boundary
//!
//! Bindings use an internal [`invalidation::InvalidationTracker`] keyed by
//! [`BindingId`]. A host calls [`BindingSet::mark_source_changed`] when an
//! endpoint changes. [`BindingSet::drain`] then evaluates dirty bindings in
//! dependency order and returns the application channels affected by target
//! writes.
//!
//! The host remains responsible for marking its own application-level
//! invalidation tracker with those returned channels.
//!
//! ## Minimal example
//!
//! ```rust
//! use invalidation::{Channel, ChannelSet};
//! use understory_binding::{
//!     BindingHost, BindingSet, BindingWrite, EndpointKey, PropertyEndpoint,
//! };
//! use understory_property::{ErasedValue, PropertyMetadataBuilder, PropertyRegistry};
//!
//! const BINDING: Channel = Channel::new(0);
//! const LAYOUT: Channel = Channel::new(1);
//!
//! struct Host {
//!     source: ErasedValue,
//!     target: Option<ErasedValue>,
//! }
//!
//! impl BindingHost<u32> for Host {
//!     fn get_erased(&self, endpoint: EndpointKey<u32>) -> Option<ErasedValue> {
//!         match endpoint.owner() {
//!             1 => Some(self.source.clone()),
//!             _ => None,
//!         }
//!     }
//!
//!     fn set_erased(&mut self, endpoint: EndpointKey<u32>, value: ErasedValue) -> BindingWrite {
//!         if endpoint.owner() == 2 {
//!             self.target = Some(value);
//!             BindingWrite::new(true, LAYOUT.into_set())
//!         } else {
//!             BindingWrite::unchanged()
//!         }
//!     }
//! }
//!
//! let mut registry = PropertyRegistry::new();
//! let width = registry.register("Width", PropertyMetadataBuilder::new(0_u32).build());
//!
//! let mut bindings = BindingSet::new(BINDING);
//! bindings
//!     .bind(
//!         PropertyEndpoint::new(1, width),
//!         PropertyEndpoint::new(2, width),
//!     )
//!     .unwrap();
//!
//! let mut host = Host {
//!     source: ErasedValue::new(42_u32),
//!     target: None,
//! };
//!
//! bindings.mark_source_changed(PropertyEndpoint::new(1, width));
//! let report = bindings.drain(&mut host).unwrap();
//!
//! assert_eq!(report.evaluated_bindings(), 1);
//! assert!(report.affected_channels().contains(LAYOUT));
//! assert_eq!(
//!     host.target.as_ref().and_then(ErasedValue::downcast_ref::<u32>),
//!     Some(&42),
//! );
//! ```

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::any::TypeId;
use core::fmt;
use core::hash::Hash;

use hashbrown::{HashMap, HashSet};
use invalidation::{Channel, ChannelSet, CycleHandling, InvalidationTracker};
use understory_property::{ErasedValue, Property, PropertyId};

/// Identifier for a registered binding.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingId(u32);

impl BindingId {
    /// Creates a binding id from a raw integer.
    ///
    /// This is primarily useful for tests and diagnostics. [`BindingSet`]
    /// assigns ids when bindings are registered.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw integer id.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    fn index(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }
}

/// Untyped key for one property endpoint on one host object.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EndpointKey<K> {
    owner: K,
    property: PropertyId,
}

impl<K> EndpointKey<K> {
    /// Creates an untyped endpoint key.
    #[must_use]
    pub const fn new(owner: K, property: PropertyId) -> Self {
        Self { owner, property }
    }

    /// Returns the property id for this endpoint.
    #[must_use]
    pub const fn property(&self) -> PropertyId {
        self.property
    }
}

impl<K: Copy> EndpointKey<K> {
    /// Returns the host-defined owner key for this endpoint.
    #[must_use]
    pub const fn owner(self) -> K {
        self.owner
    }
}

/// Typed endpoint for one [`Property`] on one host object.
pub struct PropertyEndpoint<K, T> {
    owner: K,
    property: Property<T>,
}

impl<K, T> PropertyEndpoint<K, T> {
    /// Creates a typed property endpoint.
    #[must_use]
    pub const fn new(owner: K, property: Property<T>) -> Self {
        Self { owner, property }
    }

    /// Returns the property handle for this endpoint.
    #[must_use]
    pub const fn property(&self) -> Property<T> {
        self.property
    }
}

impl<K: Copy, T> PropertyEndpoint<K, T> {
    /// Returns the host-defined owner key for this endpoint.
    #[must_use]
    pub const fn owner(self) -> K {
        self.owner
    }

    /// Erases the value type and returns the runtime endpoint key.
    #[must_use]
    pub const fn key(self) -> EndpointKey<K> {
        EndpointKey::new(self.owner, self.property.id())
    }
}

impl<K: Copy, T> Copy for PropertyEndpoint<K, T> {}

impl<K: Copy, T> Clone for PropertyEndpoint<K, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K: fmt::Debug, T> fmt::Debug for PropertyEndpoint<K, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PropertyEndpoint")
            .field("owner", &self.owner)
            .field("property", &self.property.id())
            .field("value_type", &core::any::type_name::<T>())
            .finish()
    }
}

/// Result of writing a binding target endpoint.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BindingWrite {
    changed: bool,
    affected_channels: ChannelSet,
}

impl BindingWrite {
    /// Creates a write result.
    ///
    /// `changed` reports whether the target endpoint's observable value changed.
    /// `affected_channels` reports the host application channels dirtied by that
    /// change.
    #[must_use]
    pub const fn new(changed: bool, affected_channels: ChannelSet) -> Self {
        Self {
            changed,
            affected_channels,
        }
    }

    /// Creates a write result for an unchanged target endpoint.
    #[must_use]
    pub const fn unchanged() -> Self {
        Self::new(false, ChannelSet::empty())
    }

    /// Creates a write result for a changed target endpoint.
    #[must_use]
    pub const fn changed(affected_channels: ChannelSet) -> Self {
        Self::new(true, affected_channels)
    }

    /// Returns whether the target endpoint's observable value changed.
    #[must_use]
    pub const fn did_change(self) -> bool {
        self.changed
    }

    /// Returns the application channels dirtied by the write.
    #[must_use]
    pub const fn affected_channels(self) -> ChannelSet {
        self.affected_channels
    }
}

/// Summary returned by [`BindingSet::drain`].
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct BindingReport {
    evaluated_bindings: usize,
    changed_bindings: usize,
    affected_channels: ChannelSet,
}

impl BindingReport {
    /// Returns the number of binding evaluators that ran.
    #[must_use]
    pub const fn evaluated_bindings(self) -> usize {
        self.evaluated_bindings
    }

    /// Returns the number of binding target writes that changed observable values.
    #[must_use]
    pub const fn changed_bindings(self) -> usize {
        self.changed_bindings
    }

    /// Returns the union of application channels affected by binding target writes.
    #[must_use]
    pub const fn affected_channels(self) -> ChannelSet {
        self.affected_channels
    }

    fn record(&mut self, write: BindingWrite) {
        self.evaluated_bindings += 1;
        if write.did_change() {
            self.changed_bindings += 1;
        }
        self.affected_channels |= write.affected_channels();
    }
}

/// Error produced while registering or evaluating bindings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingError<K> {
    /// The binding would directly bind an endpoint to itself.
    SelfBinding {
        /// The endpoint used as both source and target.
        endpoint: EndpointKey<K>,
    },
    /// Adding the binding would introduce a cycle in the binding dependency graph.
    Cycle {
        /// The binding that would depend on another binding.
        dependent: BindingId,
        /// The binding that would be depended on.
        dependency: BindingId,
    },
    /// The binding set has reached the maximum representable binding id.
    TooManyBindings,
    /// The host did not provide a source value for the endpoint.
    MissingSource {
        /// The source endpoint that could not be read.
        endpoint: EndpointKey<K>,
    },
    /// The host returned a source value with a different runtime type.
    SourceTypeMismatch {
        /// The source endpoint that returned the wrong value type.
        endpoint: EndpointKey<K>,
        /// The type expected by the binding declaration.
        expected: TypeId,
        /// The type returned by the host.
        actual: TypeId,
    },
}

/// Object-safe host boundary used by binding evaluation.
///
/// Hosts decide how endpoint keys map to their property stores and how target
/// writes produce application invalidation channels.
pub trait BindingHost<K: Copy> {
    /// Reads an endpoint as an erased value.
    fn get_erased(&self, endpoint: EndpointKey<K>) -> Option<ErasedValue>;

    /// Writes an erased target value and reports whether it changed.
    fn set_erased(&mut self, endpoint: EndpointKey<K>, value: ErasedValue) -> BindingWrite;
}

/// Typed convenience methods for [`BindingHost`].
pub trait BindingHostExt<K: Copy>: BindingHost<K> {
    /// Reads and downcasts an endpoint value.
    #[must_use]
    fn get<T: Clone + 'static>(&self, endpoint: PropertyEndpoint<K, T>) -> Option<T> {
        self.get_erased(endpoint.key())
            .and_then(|value| value.downcast_ref::<T>().cloned())
    }

    /// Writes a typed target endpoint value.
    fn set<T: Clone + 'static>(
        &mut self,
        endpoint: PropertyEndpoint<K, T>,
        value: T,
    ) -> BindingWrite {
        self.set_erased(endpoint.key(), ErasedValue::new(value))
    }
}

impl<K, H> BindingHostExt<K> for H
where
    K: Copy,
    H: BindingHost<K> + ?Sized,
{
}

trait ErasedBinding<K: Copy> {
    fn source(&self) -> EndpointKey<K>;
    fn target(&self) -> EndpointKey<K>;
    fn evaluate(&self, host: &mut dyn BindingHost<K>) -> Result<BindingWrite, BindingError<K>>;
}

struct TypedBinding<K, S, T> {
    source: PropertyEndpoint<K, S>,
    target: PropertyEndpoint<K, T>,
    map: Box<dyn Fn(&S) -> T>,
}

impl<K, S, T> ErasedBinding<K> for TypedBinding<K, S, T>
where
    K: Copy,
    S: Clone + 'static,
    T: Clone + 'static,
{
    fn source(&self) -> EndpointKey<K> {
        self.source.key()
    }

    fn target(&self) -> EndpointKey<K> {
        self.target.key()
    }

    fn evaluate(&self, host: &mut dyn BindingHost<K>) -> Result<BindingWrite, BindingError<K>> {
        let source = self.source();
        let erased = host
            .get_erased(source)
            .ok_or(BindingError::MissingSource { endpoint: source })?;
        let source_value =
            erased
                .downcast_ref::<S>()
                .ok_or_else(|| BindingError::SourceTypeMismatch {
                    endpoint: source,
                    expected: TypeId::of::<S>(),
                    actual: erased.type_id(),
                })?;
        let target_value = (self.map)(source_value);
        Ok(host.set_erased(self.target(), ErasedValue::new(target_value)))
    }
}

/// Registered one-way bindings and their dirty state.
///
/// The set stores bindings, endpoint indexes, and a binding-local invalidation
/// graph. It does not store property values; values are read from and written to
/// a host passed to [`Self::drain`].
pub struct BindingSet<K>
where
    K: Copy + Eq + Hash + 'static,
{
    binding_channel: Channel,
    bindings: Vec<Box<dyn ErasedBinding<K>>>,
    source_index: HashMap<EndpointKey<K>, Vec<BindingId>>,
    target_index: HashMap<EndpointKey<K>, Vec<BindingId>>,
    tracker: InvalidationTracker<u32>,
}

impl<K> BindingSet<K>
where
    K: Copy + Eq + Hash + 'static,
{
    /// Creates an empty binding set.
    ///
    /// The channel is used only inside this binding set's tracker. It does not
    /// reserve or define an application-level invalidation channel.
    #[must_use]
    pub fn new(binding_channel: Channel) -> Self {
        Self {
            binding_channel,
            bindings: Vec::new(),
            source_index: HashMap::new(),
            target_index: HashMap::new(),
            tracker: InvalidationTracker::with_cycle_handling(CycleHandling::Error),
        }
    }

    /// Returns the binding-local invalidation channel.
    #[must_use]
    pub const fn binding_channel(&self) -> Channel {
        self.binding_channel
    }

    /// Returns the number of registered bindings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Returns `true` when no bindings are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Returns `true` when there are dirty bindings waiting to be drained.
    #[must_use]
    pub fn has_dirty_bindings(&self) -> bool {
        self.tracker.has_invalidated(self.binding_channel)
    }

    /// Registers a one-way identity binding.
    ///
    /// The source and target endpoint value types must match. Use
    /// [`Self::bind_map`] when the target type differs or a conversion is needed.
    pub fn bind<T>(
        &mut self,
        source: PropertyEndpoint<K, T>,
        target: PropertyEndpoint<K, T>,
    ) -> Result<BindingId, BindingError<K>>
    where
        T: Clone + 'static,
    {
        self.bind_map(source, target, T::clone)
    }

    /// Registers a one-way mapped binding.
    ///
    /// `map` runs when the source endpoint is dirty and produces the value to
    /// write to the target endpoint.
    pub fn bind_map<S, T, F>(
        &mut self,
        source: PropertyEndpoint<K, S>,
        target: PropertyEndpoint<K, T>,
        map: F,
    ) -> Result<BindingId, BindingError<K>>
    where
        S: Clone + 'static,
        T: Clone + 'static,
        F: Fn(&S) -> T + 'static,
    {
        let source_key = source.key();
        let target_key = target.key();
        if source_key == target_key {
            return Err(BindingError::SelfBinding {
                endpoint: source_key,
            });
        }

        let raw = u32::try_from(self.bindings.len()).map_err(|_| BindingError::TooManyBindings)?;
        let id = BindingId::new(raw);
        let dependencies = self.binding_dependencies(source_key, target_key, id);

        let mut tracker = self.tracker.clone();
        for (dependent, dependency) in dependencies {
            tracker
                .add_dependency(dependent.get(), dependency.get(), self.binding_channel)
                .map_err(|_| BindingError::Cycle {
                    dependent,
                    dependency,
                })?;
        }
        self.tracker = tracker;

        self.bindings.push(Box::new(TypedBinding {
            source,
            target,
            map: Box::new(map),
        }));
        self.source_index.entry(source_key).or_default().push(id);
        self.target_index.entry(target_key).or_default().push(id);

        Ok(id)
    }

    /// Marks all bindings that read `source` as dirty.
    ///
    /// Hosts should call this after an external write changes an endpoint's
    /// observable value.
    pub fn mark_source_changed<T>(&mut self, source: PropertyEndpoint<K, T>) -> bool {
        self.mark_endpoint_changed(source.key())
    }

    /// Marks all bindings that read `source` as dirty using an untyped endpoint key.
    pub fn mark_endpoint_changed(&mut self, source: EndpointKey<K>) -> bool {
        self.mark_endpoint_changed_skipping(source, None)
    }

    /// Marks a specific binding as dirty.
    ///
    /// Returns `false` when the binding id is not registered.
    pub fn mark_binding_dirty(&mut self, binding: BindingId) -> bool {
        if binding
            .index()
            .is_none_or(|index| index >= self.bindings.len())
        {
            return false;
        }
        self.tracker.mark(binding.get(), self.binding_channel)
    }

    /// Evaluates dirty bindings until the binding set is clean.
    ///
    /// Bindings dirtied at the same time are evaluated in dependency order.
    /// When a binding target changes, bindings that read that target are marked
    /// dirty for a later pass unless they are already scheduled in the current
    /// pass.
    pub fn drain<H>(&mut self, host: &mut H) -> Result<BindingReport, BindingError<K>>
    where
        H: BindingHost<K>,
    {
        let mut report = BindingReport::default();

        while self.tracker.has_invalidated(self.binding_channel) {
            let raw_bindings: Vec<_> = self.tracker.drain_sorted(self.binding_channel).collect();
            let mut current_batch = HashSet::with_capacity(raw_bindings.len());
            current_batch.extend(raw_bindings.iter().copied());

            for raw in raw_bindings {
                let Some((target, write)) = self.evaluate_raw(raw, host)? else {
                    continue;
                };
                report.record(write);
                if write.did_change() {
                    self.mark_endpoint_changed_skipping(target, Some(&current_batch));
                }
            }
        }

        Ok(report)
    }

    fn binding_dependencies(
        &self,
        source: EndpointKey<K>,
        target: EndpointKey<K>,
        id: BindingId,
    ) -> Vec<(BindingId, BindingId)> {
        let mut dependencies = Vec::new();

        if let Some(upstream) = self.target_index.get(&source) {
            dependencies.extend(upstream.iter().copied().map(|dependency| (id, dependency)));
        }

        if let Some(downstream) = self.source_index.get(&target) {
            dependencies.extend(downstream.iter().copied().map(|dependent| (dependent, id)));
        }

        dependencies
    }

    fn mark_endpoint_changed_skipping(
        &mut self,
        source: EndpointKey<K>,
        skip: Option<&HashSet<u32>>,
    ) -> bool {
        let Some(bindings) = self.source_index.get(&source) else {
            return false;
        };
        let bindings = bindings.to_vec();
        let mut marked = false;
        for binding in bindings {
            if skip.is_some_and(|set| set.contains(&binding.get())) {
                continue;
            }
            marked |= self.tracker.mark(binding.get(), self.binding_channel);
        }
        marked
    }

    fn evaluate_raw<H>(
        &self,
        raw: u32,
        host: &mut H,
    ) -> Result<Option<(EndpointKey<K>, BindingWrite)>, BindingError<K>>
    where
        H: BindingHost<K>,
    {
        let Some(index) = usize::try_from(raw).ok() else {
            return Ok(None);
        };
        let Some(binding) = self.bindings.get(index) else {
            return Ok(None);
        };
        let target = binding.target();
        let write = binding.evaluate(host)?;
        Ok(Some((target, write)))
    }
}

impl<K> fmt::Debug for BindingSet<K>
where
    K: Copy + Eq + Hash + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BindingSet")
            .field("binding_channel", &self.binding_channel)
            .field("len", &self.len())
            .field("has_dirty_bindings", &self.has_dirty_bindings())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeMap;
    use alloc::format;
    use alloc::string::String;

    use super::*;
    use invalidation::Channel;
    use understory_property::{Property, PropertyMetadataBuilder, PropertyRegistry};

    const BINDING: Channel = Channel::new(0);
    const LAYOUT: Channel = Channel::new(1);
    const PAINT: Channel = Channel::new(2);

    #[derive(Default)]
    struct TestHost {
        values: BTreeMap<EndpointKey<u32>, ErasedValue>,
        channels: BTreeMap<PropertyId, ChannelSet>,
        writes: Vec<EndpointKey<u32>>,
    }

    impl TestHost {
        fn set_initial<T: Clone + 'static>(
            &mut self,
            endpoint: PropertyEndpoint<u32, T>,
            value: T,
        ) {
            self.values.insert(endpoint.key(), ErasedValue::new(value));
        }

        fn set_channels<T>(&mut self, endpoint: PropertyEndpoint<u32, T>, channels: ChannelSet) {
            self.channels.insert(endpoint.property().id(), channels);
        }

        fn value<T: 'static>(&self, endpoint: PropertyEndpoint<u32, T>) -> Option<&T> {
            self.values
                .get(&endpoint.key())
                .and_then(ErasedValue::downcast_ref)
        }

        fn erased_equal(left: &ErasedValue, right: &ErasedValue) -> bool {
            if left.type_id() != right.type_id() {
                return false;
            }
            if let (Some(left), Some(right)) =
                (left.downcast_ref::<u32>(), right.downcast_ref::<u32>())
            {
                return left == right;
            }
            if let (Some(left), Some(right)) = (
                left.downcast_ref::<String>(),
                right.downcast_ref::<String>(),
            ) {
                return left == right;
            }
            false
        }
    }

    impl BindingHost<u32> for TestHost {
        fn get_erased(&self, endpoint: EndpointKey<u32>) -> Option<ErasedValue> {
            self.values.get(&endpoint).cloned()
        }

        fn set_erased(&mut self, endpoint: EndpointKey<u32>, value: ErasedValue) -> BindingWrite {
            let changed = self
                .values
                .get(&endpoint)
                .is_none_or(|old| !Self::erased_equal(old, &value));
            self.values.insert(endpoint, value);
            self.writes.push(endpoint);

            let channels = if changed {
                self.channels
                    .get(&endpoint.property())
                    .copied()
                    .unwrap_or_else(ChannelSet::empty)
            } else {
                ChannelSet::empty()
            };

            BindingWrite::new(changed, channels)
        }
    }

    fn registry() -> (PropertyRegistry, Property<u32>) {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0_u32).build());
        (registry, width)
    }

    #[test]
    fn one_way_binding_copies_changed_value() {
        let (_registry, width) = registry();
        let source = PropertyEndpoint::new(1, width);
        let target = PropertyEndpoint::new(2, width);

        let mut bindings = BindingSet::new(BINDING);
        bindings.bind(source, target).unwrap();

        let mut host = TestHost::default();
        host.set_initial(source, 42_u32);
        host.set_channels(target, LAYOUT.into_set());

        assert!(bindings.mark_source_changed(source));
        let report = bindings.drain(&mut host).unwrap();

        assert_eq!(report.evaluated_bindings(), 1);
        assert_eq!(report.changed_bindings(), 1);
        assert!(report.affected_channels().contains(LAYOUT));
        assert_eq!(host.value(target), Some(&42));
    }

    #[test]
    fn drain_without_dirty_source_does_nothing() {
        let (_registry, width) = registry();
        let source = PropertyEndpoint::new(1, width);
        let target = PropertyEndpoint::new(2, width);

        let mut bindings = BindingSet::new(BINDING);
        bindings.bind(source, target).unwrap();

        let mut host = TestHost::default();
        host.set_initial(source, 42_u32);

        let report = bindings.drain(&mut host).unwrap();

        assert_eq!(report.evaluated_bindings(), 0);
        assert_eq!(host.value(target), None);
    }

    #[test]
    fn mapped_binding_converts_value_type() {
        let mut registry = PropertyRegistry::new();
        let count = registry.register("Count", PropertyMetadataBuilder::new(0_u32).build());
        let label = registry.register("Label", PropertyMetadataBuilder::new(String::new()).build());
        let source = PropertyEndpoint::new(1, count);
        let target = PropertyEndpoint::new(2, label);

        let mut bindings = BindingSet::new(BINDING);
        bindings
            .bind_map(source, target, |value| format!("count: {value}"))
            .unwrap();

        let mut host = TestHost::default();
        host.set_initial(source, 7_u32);
        host.set_channels(target, PAINT.into_set());

        bindings.mark_source_changed(source);
        let report = bindings.drain(&mut host).unwrap();

        assert_eq!(report.evaluated_bindings(), 1);
        assert!(report.affected_channels().contains(PAINT));
        assert_eq!(host.value(target).map(String::as_str), Some("count: 7"));
    }

    #[test]
    fn chained_bindings_propagate_after_target_change() {
        let (_registry, width) = registry();
        let first = PropertyEndpoint::new(1, width);
        let second = PropertyEndpoint::new(2, width);
        let third = PropertyEndpoint::new(3, width);

        let mut bindings = BindingSet::new(BINDING);
        let first_to_second = bindings.bind(first, second).unwrap();
        let second_to_third = bindings.bind(second, third).unwrap();

        let mut host = TestHost::default();
        host.set_initial(first, 10_u32);
        host.set_channels(second, LAYOUT.into_set());
        host.set_channels(third, PAINT.into_set());

        bindings.mark_source_changed(first);
        let report = bindings.drain(&mut host).unwrap();

        assert_eq!(report.evaluated_bindings(), 2);
        assert_eq!(host.value(second), Some(&10));
        assert_eq!(host.value(third), Some(&10));
        assert_eq!(
            host.writes,
            alloc::vec![second.key(), third.key()],
            "binding ids {first_to_second:?} and {second_to_third:?} should drain in dependency order",
        );
    }

    #[test]
    fn simultaneous_dirty_sources_use_dependency_order_without_duplicate_downstream_eval() {
        let (_registry, width) = registry();
        let first = PropertyEndpoint::new(1, width);
        let second = PropertyEndpoint::new(2, width);
        let third = PropertyEndpoint::new(3, width);

        let mut bindings = BindingSet::new(BINDING);
        bindings.bind(first, second).unwrap();
        bindings.bind(second, third).unwrap();

        let mut host = TestHost::default();
        host.set_initial(first, 10_u32);
        host.set_initial(second, 99_u32);

        bindings.mark_source_changed(first);
        bindings.mark_source_changed(second);
        let report = bindings.drain(&mut host).unwrap();

        assert_eq!(report.evaluated_bindings(), 2);
        assert_eq!(host.value(third), Some(&10));
        assert_eq!(host.writes, alloc::vec![second.key(), third.key()]);
    }

    #[test]
    fn cycle_is_rejected() {
        let (_registry, width) = registry();
        let first = PropertyEndpoint::new(1, width);
        let second = PropertyEndpoint::new(2, width);

        let mut bindings = BindingSet::new(BINDING);
        let first_id = bindings.bind(first, second).unwrap();
        let error = bindings.bind(second, first).unwrap_err();

        assert!(matches!(
            error,
            BindingError::Cycle {
                dependent,
                dependency
            } if dependent == first_id && dependency == BindingId::new(1)
        ));
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn direct_self_binding_is_rejected() {
        let (_registry, width) = registry();
        let endpoint = PropertyEndpoint::new(1, width);

        let mut bindings = BindingSet::new(BINDING);
        let error = bindings.bind(endpoint, endpoint).unwrap_err();

        assert!(matches!(error, BindingError::SelfBinding { .. }));
        assert!(bindings.is_empty());
    }

    #[test]
    fn source_type_mismatch_errors() {
        let (_registry, width) = registry();
        let source = PropertyEndpoint::new(1, width);
        let target = PropertyEndpoint::new(2, width);

        let mut bindings = BindingSet::new(BINDING);
        bindings.bind(source, target).unwrap();

        let mut host = TestHost::default();
        host.values
            .insert(source.key(), ErasedValue::new(String::from("wrong")));

        bindings.mark_source_changed(source);
        let error = bindings.drain(&mut host).unwrap_err();

        assert!(matches!(
            error,
            BindingError::SourceTypeMismatch { endpoint, .. } if endpoint == source.key()
        ));
    }
}
