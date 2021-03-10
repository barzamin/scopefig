use std::{fs::{self, File}, io::{BufWriter, Seek, Write}, path::PathBuf};

use hound::WavWriter;
use kv_log_macro as log;
use anyhow::{anyhow, Result};
use lyon_geom::{LineSegment, Point};
use lyon_path::{PathEvent, iterator::Flattened};
use lyon_path::Path;
use structopt::StructOpt;
use usvg::prelude::*;

mod svg;

#[derive(Debug, StructOpt)]
struct Args {
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    #[structopt(short, long, parse(from_os_str), default_value="out.wav")]
    output: PathBuf,
}

const F_s: u32 = 44_100; // Hz
const DRAW_VELOCITY: f32 = 5000.; // units/s
// const TRANSIT_VELOCITY: f32 = 40000.; // units/s
const TOLERANCE: f32 = 0.1;

fn draw_line(pts: &mut Vec<Point<f32>>, line: LineSegment<f32>) {
    log::trace!("emit line {:?}", line);

    let n_samples: usize = (F_s as f32 * line.length() / DRAW_VELOCITY).trunc() as usize;
    println!("n_samples: {}", n_samples);
    for t in (0..n_samples).map(|i| i as f32 / n_samples as f32) {
        println!("t : {}, sample : {:?}", t, line.sample(t));
        pts.push(line.sample(t));
    }
}

fn write_wav<W>(wtr: W, pts: &[Point<f32>]) -> Result<(), hound::Error> where W: Write + Seek {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: F_s,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut wav_wtr = hound::WavWriter::new(wtr, spec)?;
    for pt in pts {
        wav_wtr.write_sample(pt.x)?;
        wav_wtr.write_sample(pt.y)?;
    }
    wav_wtr.finalize()?;

    Ok(())
}

trait Bounded<T> {
    fn bounds_x(&self) -> (T, T);
    fn bounds_y(&self) -> (T, T);
}

impl Bounded<f32> for &[Point<f32>] {
    fn bounds_x(&self) -> (f32, f32) {
        todo!()
    }

    fn bounds_y(&self) -> (f32, f32) {
        todo!()
    }
}

fn main() -> Result<()> {
    femme::with_level(femme::LevelFilter::Trace);

    let args = Args::from_args();

    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_file(args.input, &opt)?;

    let mut pts: Vec<Point<f32>> = vec![];

    for node in tree.root().descendants() {
        if let usvg::NodeKind::Path(ref p) = *node.borrow() {
            println!("{:?}", node);
            let flattened = Flattened::new(TOLERANCE, svg::convert_path(p));
            for evt in flattened {
                log::trace!(" -> {:?}", evt);
                match evt {
                    lyon_path::Event::Begin { at } => {
                        // slew at full speed to point
                        if let Some(last) = pts.last() {
                            // let line = LineSegment {from: last.clone(), to: at};
                            // draw_line(&mut pts, line);
                            pts.push(at);
                        } else {
                            // let line = LineSegment {from: Point::new(0, 0), to: at};
                            pts.push(at);
                        }
                    },
                    lyon_path::Event::End { last, first, close } => {
                        if close {
                            // pts.push(first);
                            let line = LineSegment { from: last, to: first };
                            draw_line(&mut pts, line);
                        }
                    },
                    lyon_path::Event::Line { from, to } => {
                        // emit_pos(&mut wtr, to)?; // go where we're supposed to. TODO: slew speed.
                        let line = LineSegment { from, to };
                        draw_line(&mut pts, line);
                        // pts.push(to);
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

    write_wav(BufWriter::new(File::create(args.output)?), &pts)?;

    Ok(())
}
