// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Invalidation channels used by the Overstory runtime pipeline.

use invalidation::Channel;

/// Semantic state or class changes that can alter style matching.
pub const STYLE: Channel = Channel::new(0);

/// Control-template expansion or template binding inputs.
pub const TEMPLATE: Channel = Channel::new(1);

/// Intrinsic size computation.
pub const MEASURE: Channel = Channel::new(2);

/// Placement of measured content into final bounds.
pub const ARRANGE: Channel = Channel::new(3);

/// Visual lowering into `imaging`.
pub const VISUAL: Channel = Channel::new(4);
