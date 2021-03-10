use core::f32::consts::PI;
use std::cmp;
use std::io::{Lines, Seek, Write};
use std::path::PathBuf;
use std::fs;

use hound::WavWriter;
use kv_log_macro as log;
use anyhow::{anyhow, Result};
use lyon_geom::{LineSegment, Point};
use lyon_path::{PathEvent, iterator::Flattened};
use lyon_path::Path;
use structopt::StructOpt;
use usvg::prelude::*;
// use plotters::{coord::types::RangedCoordf32, prelude::*};

#[derive(Debug, StructOpt)]
struct Args {
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    #[structopt(short, long, parse(from_os_str), default_value="out.wav")]
    output: PathBuf,
}

const F_s: u32 = 44_100; // Hz
const DRAW_VELOCITY: f32 = 10000.; // units/s
const TOLERANCE: f32 = 0.1;

// fn emit_pos<W>(w: &mut WavWriter<W>, pos: Point<f32>) -> Result<()> where W: Write + Seek {
//     // log::trace!("slew to {:?}", pos);
//     w.write_sample(pos.x)?;
//     w.write_sample(pos.y)?;
//     // w.write_sample(pos.z)?;
//     Ok(())
// }

fn draw_line(pts: &mut Vec<Point<f32>>, line: LineSegment<f32>) {
    log::trace!("emit line {:?}", line);

    let n_samples: usize = 20;//(F_s as f32 * line.length() / DRAW_VELOCITY).trunc() as usize;
    println!("n_samples: {}", n_samples);
    for t in (1..n_samples).map(|i| i as f32 / n_samples as f32) {
        pts.push(line.sample(t));
    }
}

fn point(x: &f64, y: &f64) -> Point<f32> {
    Point::new(*x as f32, *y as f32)
}

pub struct PathConvIter<'a> {
    iter: std::slice::Iter<'a, usvg::PathSegment>,
    prev: Point<f32>,
    first: Point<f32>,
    needs_end: bool,
    deferred: Option<PathEvent>,
}

impl<'l> Iterator for PathConvIter<'l> {
    type Item = PathEvent;
    fn next(&mut self) -> Option<PathEvent> {
        if self.deferred.is_some() {
            return self.deferred.take();
        }

        let next = self.iter.next();
        println!("{:?}", next);
        match next {
            Some(usvg::PathSegment::MoveTo { x, y }) => {
                if self.needs_end {
                    let last = self.prev;
                    let first = self.first;
                    self.needs_end = false;
                    self.prev = point(x, y);
                    self.deferred = Some(PathEvent::Begin { at: self.prev });
                    self.first = self.prev;
                    Some(PathEvent::End {
                        last,
                        first,
                        close: false,
                    })
                } else {
                    self.first = point(x, y);
                    self.prev = self.first;
                    Some(PathEvent::Begin { at: self.first })
                }
            }
            Some(usvg::PathSegment::LineTo { x, y }) => {
                self.needs_end = true;
                let from = self.prev;
                self.prev = point(x, y);
                Some(PathEvent::Line {
                    from,
                    to: self.prev,
                })
            }
            Some(usvg::PathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            }) => {
                self.needs_end = true;
                let from = self.prev;
                self.prev = point(x, y);
                Some(PathEvent::Cubic {
                    from,
                    ctrl1: point(x1, y1),
                    ctrl2: point(x2, y2),
                    to: self.prev,
                })
            }
            Some(usvg::PathSegment::ClosePath) => {
                self.needs_end = false;
                self.prev = self.first;
                Some(PathEvent::End {
                    last: self.prev,
                    first: self.first,
                    close: true,
                })
            }
            None => {
                if self.needs_end {
                    self.needs_end = false;
                    let last = self.prev;
                    let first = self.first;
                    Some(PathEvent::End {
                        last,
                        first,
                        close: false,
                    })
                } else {
                    None
                }
            }
        }
    }
}

pub fn convert_path<'a>(p: &'a usvg::Path) -> PathConvIter<'a> {
    PathConvIter {
        iter: p.data.iter(),
        first: Point::new(0.0, 0.0),
        prev: Point::new(0.0, 0.0),
        deferred: None,
        needs_end: false,
    }
}
fn main() -> Result<()> {
    femme::with_level(femme::LevelFilter::Trace);

    let args = Args::from_args();
    let out_spec = hound::WavSpec {
        channels: 2,
        sample_rate: F_s,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut wtr = hound::WavWriter::create(args.output, out_spec)?;

    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_file(args.input, &opt)?;

    let mut pts: Vec<Point<f32>> = vec![];

    for node in tree.root().descendants() {
        if let usvg::NodeKind::Path(ref p) = *node.borrow() {
            println!("{:?}", node);
            let flattened = Flattened::new(TOLERANCE, convert_path(p));
            for evt in flattened {
                log::trace!(" -> {:?}", evt);
                match evt {
                    lyon_path::Event::Begin { at } => {
                        // slew at full speed to point
                        pts.push(at);
                    },
                    lyon_path::Event::End { last, first, close } => {
                        if close {
                            pts.push(first);
                            // let line = LineSegment { from: last, to: first };
                            // draw_line(&mut pts, line);
                            // emit_pos(&mut wtr, first)?; // loop back if closed. TODO: slew speed.
                        }
                    },
                    lyon_path::Event::Line { from, to } => {
                        // emit_pos(&mut wtr, to)?; // go where we're supposed to. TODO: slew speed.
                        // let line = LineSegment { from, to };
                        // draw_line(&mut pts, line);
                        pts.push(to);
                    },
                    _ => {
                        log::warn!("unsupported path element {:?}", evt);
                    }
                }
            }
        }
    }

    let max_x = pts.iter().map(|pt| pt.x)
        .max_by(|a, b| a.partial_cmp(b).expect("Tried to compare a NaN")).unwrap();
    let max_y = pts.iter().map(|pt| pt.y)
        .max_by(|a, b| a.partial_cmp(b).expect("Tried to compare a NaN")).unwrap();
    let min_x = pts.iter().map(|pt| pt.x)
        .min_by(|a, b| a.partial_cmp(b).expect("Tried to compare a NaN")).unwrap();
    let min_y = pts.iter().map(|pt| pt.y)
        .min_by(|a, b| a.partial_cmp(b).expect("Tried to compare a NaN")).unwrap();
    log::debug!("max x={}, y={}; min x={}, y={}", max_x, max_y, min_x, min_y);
    let width = max_x - min_x;
    let height = max_y - min_y;
    let scale = 2./width.max(height); // scale all coords by

    for pt in pts.iter_mut() {
        pt.x = (pt.x-min_x-width/2.)*scale;
        pt.y = -(pt.y-min_y-height/2.)*scale;
    }

    let max_x = pts.iter().map(|pt| pt.x)
        .max_by(|a, b| a.partial_cmp(b).expect("Tried to compare a NaN")).unwrap();
    let max_y = pts.iter().map(|pt| pt.y)
        .max_by(|a, b| a.partial_cmp(b).expect("Tried to compare a NaN")).unwrap();
    let min_x = pts.iter().map(|pt| pt.x)
        .min_by(|a, b| a.partial_cmp(b).expect("Tried to compare a NaN")).unwrap();
    let min_y = pts.iter().map(|pt| pt.y)
        .min_by(|a, b| a.partial_cmp(b).expect("Tried to compare a NaN")).unwrap();
    log::debug!("normalized! max x={}, y={}; min x={}, y={}", max_x, max_y, min_x, min_y);

    // let mut root = BitMapBackend::new("plot.png", (600, 600)).into_drawing_area();
    // root.fill(&WHITE)?;
    // let root = root.apply_coord_spec(Cartesian2d::<RangedCoordf32, RangedCoordf32>::new(
    //     // -1f32..1f32,
    //     // -1f32..1f32,
    //     min_x..max_x,
    //     min_y..max_y,
    //     (0..600, 0..600),
    // ));


    for _ in 0..1000 {
        for pt in &pts {
            // root.draw(&(EmptyElement::at((pt.x, pt.y)) + Circle::new((0, 0), 3, ShapeStyle::from(&BLACK).filled())))?;
            wtr.write_sample(pt.x)?;
            wtr.write_sample(pt.y)?;
        }
    }

    wtr.finalize()?;

    Ok(())
}
