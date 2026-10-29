use crate::drawing::{Colour, Ink, Mark, draw, draw_solution, draw_zones};
use crate::maze::Maze;
use std::fmt::Write;

const CELL: f64 = 20.0;

const ZONE_COLOURS: [Colour; 5] = [
    Colour {
        red: 0.84,
        green: 0.16,
        blue: 0.16,
    },
    Colour {
        red: 0.16,
        green: 0.62,
        blue: 0.56,
    },
    Colour {
        red: 0.23,
        green: 0.36,
        blue: 0.85,
    },
    Colour {
        red: 0.96,
        green: 0.64,
        blue: 0.38,
    },
    Colour {
        red: 0.51,
        green: 0.22,
        blue: 0.93,
    },
];

pub fn render(maze: &Maze) -> String {
    svg(maze, &draw(maze, Ink::Colour))
}

pub fn render_solution(maze: &Maze) -> String {
    let mut marks = draw(maze, Ink::Colour);
    marks.extend(draw_solution(maze, Ink::Colour));
    svg(maze, &marks)
}

pub fn render_zones(maze: &Maze) -> String {
    svg(maze, &draw_zones(maze, &ZONE_COLOURS))
}

fn svg(maze: &Maze, marks: &[Mark]) -> String {
    let width = maze.grid.width as f64 * CELL;
    let height = maze.grid.height as f64 * CELL;
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><rect width="100%" height="100%" fill="white"/>"#
    );
    for mark in marks {
        match mark.moved(CELL, |(x, y)| (x * CELL, y * CELL)) {
            Mark::Lines {
                lines,
                width,
                colour,
                dash,
                round,
            } => {
                let (cap, join) = if round {
                    ("round", "round")
                } else {
                    ("butt", "miter")
                };
                let pattern = dash.map_or(String::new(), |[on, off]| {
                    format!(r#" stroke-dasharray="{on} {off}""#)
                });
                let d: String = lines
                    .iter()
                    .map(|line| {
                        line.iter()
                            .enumerate()
                            .map(|(i, (x, y))| format!("{}{x} {y}", if i == 0 { 'M' } else { 'L' }))
                            .collect::<String>()
                    })
                    .collect();
                write!(
                    svg,
                    r#"<path d="{d}" fill="none" stroke="{}" stroke-width="{width}" stroke-linecap="{cap}" stroke-linejoin="{join}"{pattern}/>"#,
                    hex(colour)
                )
                .unwrap();
            }
            Mark::Disc {
                centre: (x, y),
                radius,
                colour,
            } => write!(
                svg,
                r#"<circle cx="{x}" cy="{y}" r="{radius}" fill="{}"/>"#,
                hex(colour)
            )
            .unwrap(),
            Mark::Polygon { points, colour } => {
                let corners: Vec<String> = points.iter().map(|(x, y)| format!("{x},{y}")).collect();
                write!(
                    svg,
                    r#"<polygon points="{}" fill="{}"/>"#,
                    corners.join(" "),
                    hex(colour)
                )
                .unwrap();
            }
        }
    }
    svg.push_str("</svg>");
    svg
}

fn hex(colour: Colour) -> String {
    let byte = |level: f64| (level * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        byte(colour.red),
        byte(colour.green),
        byte(colour.blue)
    )
}
