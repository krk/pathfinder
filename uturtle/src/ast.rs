// pathfinder/renderer/src/tile_map.rs
//
// Copyright © 2019 The Pathfinder Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Inspiration for the commands are from https://docs.kde.org/trunk5/en/kdeedu/kturtle/commands.html

#[derive(Debug)]
pub enum Command {
    Reset,
    PenUp,
    PenDown,
    Turn(f32),
    Move(f32),
    Direction(f32),
    PushLoc,
    PopLoc,
    PushRot,
    PopRot,
    Go(f32, f32),
    GoX(f32),
    GoY(f32),
    PenWidth(f32),
    PenColor(u8, u8, u8), // RGB color.
}

pub type Turtle = Vec<Command>;
