// pathfinder/renderer/src/tile_map.rs
//
// Copyright © 2019 The Pathfinder Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[macro_use]
extern crate lalrpop_util;

pub mod ast;

lalrpop_mod!(pub turtle); // synthesized by LALRPOP

pub type Parser = turtle::TopLevelParser;

#[test]
fn turtle_command_parser() {
    assert!(turtle::CommandParser::new().parse("penup").is_ok());
    assert!(turtle::CommandParser::new().parse("pendown").is_ok());
    assert!(turtle::CommandParser::new().parse("turnleft").is_ok());
    assert!(turtle::CommandParser::new().parse("turnleft 22.7").is_ok());
    assert!(turtle::CommandParser::new().parse("turnright").is_ok());
    assert!(turtle::CommandParser::new().parse("turnright 12.3").is_ok());
    assert!(turtle::CommandParser::new().parse("pushloc").is_ok());
    assert!(turtle::CommandParser::new().parse("poploc").is_ok());
    assert!(turtle::CommandParser::new().parse("pushrot").is_ok());
    assert!(turtle::CommandParser::new().parse("poprot").is_ok());
    assert!(turtle::CommandParser::new().parse("go 1 3").is_ok());
    assert!(turtle::CommandParser::new().parse("gox 5.3").is_ok());
    assert!(turtle::CommandParser::new().parse("goy 44.2").is_ok());
    assert!(turtle::CommandParser::new().parse("penwidth 2").is_ok());
    assert!(turtle::CommandParser::new()
        .parse("pencolor 255,128 ,    128")
        .is_ok());

    assert!(turtle::CommandParser::new().parse("bleh").is_err());
    assert!(turtle::CommandParser::new().parse("penup pendown").is_err());
    assert!(turtle::CommandParser::new().parse("pushloc 22").is_err());
    assert!(turtle::CommandParser::new()
        .parse("pencolor 255,128")
        .is_err());
}

#[test]
fn turtle_program_parser() {
    assert!(turtle::TopLevelParser::new()
        .parse("turnright turnright 12.3 turnleft")
        .is_ok());
}
