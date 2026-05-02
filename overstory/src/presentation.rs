// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Presentation tree produced by template expansion and layout.

use alloc::vec::Vec;

use kurbo::Rect;
use peniko::Brush;

use kurbo::Insets;

use crate::{ElementId, PartKind, TextContent, TextStyle};

/// Stable identifier for a node in a [`PresentationTree`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PresentationNodeId(u32);

impl PresentationNodeId {
    /// Creates a presentation node identifier from a raw dense index.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw dense index.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A laid-out node produced by semantic template expansion.
#[derive(Clone, Debug, PartialEq)]
pub struct PresentationNode {
    /// Semantic element that produced this presentation node.
    pub source: ElementId,
    /// Open part kind for this presentation node.
    pub kind: PartKind,
    /// Arranged bounds in the UI coordinate space.
    pub bounds: Rect,
    /// Optional background brush to emit during visual lowering.
    pub background: Option<Brush>,
    /// Optional border brush to stroke during visual lowering.
    pub border: Option<Brush>,
    /// Border stroke width in logical UI coordinates.
    pub border_width: f64,
    /// Optional foreground brush inherited or bound from the semantic element.
    pub foreground: Option<Brush>,
    /// Optional padding bound into this presentation node.
    pub padding: Option<Insets>,
    /// Corner radius for background fills.
    pub corner_radius: f64,
    /// Optional text carried by a content presenter.
    pub text: Option<TextContent>,
    /// Text styling used when lowering text.
    pub text_style: TextStyle,
    /// Child presentation nodes.
    pub children: Vec<PresentationNodeId>,
}

impl PresentationNode {
    /// Creates a presentation node.
    #[must_use]
    pub fn new(source: ElementId, kind: PartKind, bounds: Rect) -> Self {
        Self {
            source,
            kind,
            bounds,
            background: None,
            border: None,
            border_width: 0.0,
            foreground: None,
            padding: None,
            corner_radius: 0.0,
            text: None,
            text_style: TextStyle::default(),
            children: Vec::new(),
        }
    }
}

/// Retained result of template expansion and layout.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PresentationTree {
    nodes: Vec<PresentationNode>,
    root: Option<PresentationNodeId>,
}

impl PresentationTree {
    /// Creates an empty presentation tree.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the root presentation node, if present.
    #[must_use]
    pub const fn root(&self) -> Option<PresentationNodeId> {
        self.root
    }

    /// Returns all presentation nodes in dense storage order.
    #[must_use]
    pub fn nodes(&self) -> &[PresentationNode] {
        &self.nodes
    }

    /// Returns a presentation node by identifier.
    #[must_use]
    pub fn node(&self, id: PresentationNodeId) -> Option<&PresentationNode> {
        self.nodes.get(id.index())
    }

    pub(crate) fn push(&mut self, node: PresentationNode) -> PresentationNodeId {
        let id = PresentationNodeId::from_raw(
            u32::try_from(self.nodes.len()).expect("presentation node count should fit in u32"),
        );
        if self.root.is_none() {
            self.root = Some(id);
        }
        self.nodes.push(node);
        id
    }

    pub(crate) fn push_child(
        &mut self,
        parent: PresentationNodeId,
        node: PresentationNode,
    ) -> PresentationNodeId {
        let id = self.push(node);
        self.nodes[parent.index()].children.push(id);
        id
    }
}
