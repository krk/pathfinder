// pathfinder/svg/src/lib.rs
//
// Copyright © 2019 The Pathfinder Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Converts a sequence of Turtle commands to a Pathfinder scene.

#[macro_use]
extern crate bitflags;

use std::dbg;

use pathfinder_geometry::basic::line_segment::LineSegmentF32;
use pathfinder_geometry::basic::point::Point2DF32;
use pathfinder_geometry::basic::rect::RectF32;
use pathfinder_geometry::color::ColorU;
use pathfinder_geometry::outline::Outline;
use pathfinder_geometry::segment::{Segment, SegmentFlags};
use pathfinder_geometry::stroke::OutlineStrokeToFill;
use pathfinder_renderer::scene::{Paint, PathObject, PathObjectKind, Scene};
use std::fmt::{Display, Formatter, Result as FormatResult};
use std::mem;
use uturtle::ast::Command;
use uturtle::ast::Turtle;

const HAIRLINE_STROKE_WIDTH: f32 = 0.0333;

#[derive(Debug)]
pub struct BuiltTurtle {
    pub scene: Scene,
    pub result_flags: BuildResultFlags,
    state: TurtleState,
    id_counter: u32,
}

#[derive(Debug)]
struct TurtleState {
    pos_x: f32,
    pos_y: f32,
    direction: f32,
    pen_down: bool,
    positions: Vec<(f32, f32)>,
    directions: Vec<f32>,
    pen_width: f32,
    pen_color: (u8, u8, u8),
    bounds: RectF32,
}

impl TurtleState {
    pub fn new() -> TurtleState {
        TurtleState {
            pos_x: 0.0,
            pos_y: 0.0,
            direction: 0.0,
            pen_down: false,
            positions: Vec::new(),
            directions: Vec::new(),
            pen_width: 1.0,
            pen_color: (0, 0, 0),
            bounds: RectF32::new(Point2DF32::new(0.0, 0.0), Point2DF32::new(0.0, 0.0)),
        }
    }
}

bitflags! {
    // NB: If you change this, make sure to update the `Display`
    // implementation as well.
    pub struct BuildResultFlags: u16 {
        const ERR_UNHANDLED_COMMAND       = 0x0001;
        const ERR_POPLOC_EMPTY_STACK       = 0x0002;
        const ERR_POPROT_EMPTY_STACK       = 0x0004;
    }
}

impl Display for BuildResultFlags {
    fn fmt(&self, formatter: &mut Formatter) -> FormatResult {
        if self.is_empty() {
            return Ok(());
        }

        let mut first = true;
        for (bit, name) in NAMES.iter().enumerate() {
            if (self.bits() >> bit) & 1 == 0 {
                continue;
            }
            if !first {
                formatter.write_str(", ")?;
            } else {
                first = false;
            }
            formatter.write_str(name)?;
        }

        return Ok(());

        // Must match the order in `BuildResultFlags`.
        static NAMES: &'static [&'static str] = &[
            "unhandled command",
            "poploc on empty stack",
            "poprot on empty stack",
        ];
    }
}

impl BuiltTurtle {
    pub fn from_ast(t: Turtle) -> BuiltTurtle {
        let mut built = BuiltTurtle {
            id_counter: 0,
            scene: Scene::new(),
            result_flags: BuildResultFlags::empty(),
            state: TurtleState::new(),
        };

        built.process_turtle(&t);

        // FIXME(pcwalton): This is needed to avoid stack exhaustion in debug builds when
        // recursively dropping reference counts on very large SVGs. :(
        mem::forget(t);

        built
    }

    fn id(&mut self) -> u32 {
        self.id_counter += 1;
        self.id_counter
    }

    fn update_bounds(&mut self, x: f32, y: f32) {
        self.state.bounds = self.state.bounds.union_point(Point2DF32::new(x, y));
        self.scene.bounds = self.scene.bounds.union_rect(self.state.bounds);
    }

    fn process_turtle(&mut self, t: &Turtle) {
        for cmd in t {
            match cmd {
                Command::Reset => {
                    self.state = TurtleState::new();
                    self.scene = Scene::new();
                    self.result_flags = BuildResultFlags::empty();
                }
                Command::PenUp => self.state.pen_down = false,
                Command::PenDown => self.state.pen_down = true,
                Command::Turn(deg) => {
                    self.state.direction = (((self.state.direction + *deg) % 360.0) + 360.0) % 360.0
                }
                Command::Direction(deg) => self.state.direction = ((*deg % 360.0) + 360.0) % 360.0,
                Command::Move(unit) => {
                    let (s, c) = self.state.direction.to_radians().sin_cos();
                    let to_x = self.state.pos_x + unit * c;
                    let to_y = self.state.pos_y + unit * s;

                    if self.state.pen_down {
                        self.line_to(self.state.pos_x, self.state.pos_y, to_x, to_y);
                        self.update_bounds(to_x, to_y);
                    }

                    self.state.pos_x = to_x;
                    self.state.pos_y = to_y;
                }
                Command::PushLoc => self
                    .state
                    .positions
                    .push((self.state.pos_x, self.state.pos_y)),
                Command::PopLoc => {
                    match self.state.positions.pop() {
                        Some((x, y)) => {
                            self.state.pos_x = x;
                            self.state.pos_y = y;
                        }
                        None => {
                            // NOOP.
                            eprintln!("poploc on empty stack");
                        }
                    }
                }
                Command::PushRot => self.state.directions.push(self.state.direction),
                Command::PopRot => {
                    match self.state.directions.pop() {
                        Some(deg) => {
                            self.state.direction = deg;
                        }
                        None => {
                            // NOOP.
                            eprintln!("poprot on empty stack");
                        }
                    }
                }
                Command::Go(x, y) => {
                    self.state.pos_x = *x;
                    self.state.pos_y = *y;
                    self.update_bounds(*x, *y);
                }
                Command::GoX(x) => {
                    self.state.pos_x = *x;
                    self.update_bounds(*x, self.state.pos_y);
                }
                Command::GoY(y) => {
                    self.state.pos_y = *y;
                    self.update_bounds(self.state.pos_x, *y);
                }
                Command::PenWidth(w) => {
                    self.state.pen_width = *w;
                }
                Command::PenColor(r, g, b) => {
                    self.state.pen_color = (*r, *g, *b);
                }
            }
        }

        self.scene.view_box = self.scene.bounds;
    }

    fn line_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        let style = self
            .scene
            .push_paint(&Paint::from_pencolor(self.state.pen_color));
        let stroke_width = f32::max(self.state.pen_width, HAIRLINE_STROKE_WIDTH);

        let p1 = Point2DF32::new(x1, y1);
        let p2 = Point2DF32::new(x2, y2);
        let line_segment = LineSegmentF32::new(&p1, &p2);
        let mut segment = Segment::line(&line_segment);
        segment.flags = SegmentFlags::FIRST_IN_SUBPATH;

        let segments = vec![segment].into_iter();

        let outline = Outline::from_segments(segments);

        let mut stroke_to_fill = OutlineStrokeToFill::new(outline, stroke_width);
        stroke_to_fill.offset();
        let outline = stroke_to_fill.outline;

        self.scene.bounds = self.scene.bounds.union_rect(outline.bounds());

        let id = self.id().to_string();
        self.scene.objects.push(PathObject::new(
            outline,
            style,
            id,
            PathObjectKind::Stroke,
        ));
    }
}

trait PaintExt {
    fn from_rgb(r: u8, g: u8, b: u8) -> Self;
    fn from_pencolor(pencolor: (u8, u8, u8)) -> Self;
}

impl PaintExt for Paint {
    #[inline]
    fn from_pencolor(pencolor: (u8, u8, u8)) -> Paint {
        Self::from_rgb(pencolor.0, pencolor.1, pencolor.2)
    }

    #[inline]
    fn from_rgb(r: u8, g: u8, b: u8) -> Paint {
        Paint {
            color: ColorU {
                r: r,
                g: g,
                b: b,
                a: 255,
            },
        }
    }
}
