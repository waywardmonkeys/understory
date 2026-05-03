// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Presentation tree produced by template expansion and layout.

use alloc::vec::Vec;

use kurbo::Rect;
use peniko::Brush;

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

/// Resolved visual primitive carried by a presentation node.
#[derive(Clone, Debug, PartialEq)]
pub enum PresentationPrimitive {
    /// Fill and/or stroke a rectangular surface.
    Surface(SurfacePrimitive),
    /// Draw shaped text.
    Text(TextPrimitive),
}

/// Surface fill/stroke data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfacePrimitive {
    /// Optional fill brush.
    pub background: Option<Brush>,
    /// Optional stroke brush.
    pub border: Option<Brush>,
    /// Border stroke width in logical UI coordinates.
    pub border_width: f64,
    /// Corner radius for fill and stroke.
    pub corner_radius: f64,
}

/// Text drawing data.
#[derive(Clone, Debug, PartialEq)]
pub struct TextPrimitive {
    /// Text content to draw.
    pub content: TextContent,
    /// Optional foreground brush.
    pub foreground: Option<Brush>,
    /// Text styling used when lowering text.
    pub style: TextStyle,
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
    /// Visual primitives emitted by this node.
    pub primitives: Vec<PresentationPrimitive>,
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
            primitives: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Creates a presentation node with one surface primitive.
    #[must_use]
    pub fn surface(
        source: ElementId,
        kind: PartKind,
        bounds: Rect,
        surface: SurfacePrimitive,
    ) -> Self {
        let mut node = Self::new(source, kind, bounds);
        node.primitives
            .push(PresentationPrimitive::Surface(surface));
        node
    }

    /// Creates a presentation node with one text primitive.
    #[must_use]
    pub fn text(source: ElementId, kind: PartKind, bounds: Rect, text: TextPrimitive) -> Self {
        let mut node = Self::new(source, kind, bounds);
        node.primitives.push(PresentationPrimitive::Text(text));
        node
    }

    /// Returns the first surface primitive, if any.
    #[must_use]
    pub fn surface_primitive(&self) -> Option<&SurfacePrimitive> {
        self.primitives
            .iter()
            .find_map(|primitive| match primitive {
                PresentationPrimitive::Surface(surface) => Some(surface),
                PresentationPrimitive::Text(_) => None,
            })
    }

    /// Returns the first text primitive, if any.
    #[must_use]
    pub fn text_primitive(&self) -> Option<&TextPrimitive> {
        self.primitives
            .iter()
            .find_map(|primitive| match primitive {
                PresentationPrimitive::Surface(_) => None,
                PresentationPrimitive::Text(text) => Some(text),
            })
    }

    pub(crate) fn surface_primitive_mut(&mut self) -> &mut SurfacePrimitive {
        if let Some(index) = self
            .primitives
            .iter()
            .position(|primitive| matches!(primitive, PresentationPrimitive::Surface(_)))
        {
            let PresentationPrimitive::Surface(surface) = &mut self.primitives[index] else {
                unreachable!("matched surface primitive");
            };
            return surface;
        }
        self.primitives
            .push(PresentationPrimitive::Surface(SurfacePrimitive::default()));
        let Some(PresentationPrimitive::Surface(surface)) = self.primitives.last_mut() else {
            unreachable!("just pushed surface primitive");
        };
        surface
    }

    pub(crate) fn text_primitive_mut(&mut self) -> &mut TextPrimitive {
        if let Some(index) = self
            .primitives
            .iter()
            .position(|primitive| matches!(primitive, PresentationPrimitive::Text(_)))
        {
            let PresentationPrimitive::Text(text) = &mut self.primitives[index] else {
                unreachable!("matched text primitive");
            };
            return text;
        }
        self.primitives
            .push(PresentationPrimitive::Text(TextPrimitive {
                content: TextContent::default(),
                foreground: None,
                style: TextStyle::default(),
            }));
        let Some(PresentationPrimitive::Text(text)) = self.primitives.last_mut() else {
            unreachable!("just pushed text primitive");
        };
        text
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
