use core::f32::consts::PI;
use std::cmp;
use std::io::{Lines, Seek, Write};
use std::path::PathBuf;
use std::fs;

use hound::WavWriter;
use kv_log_macro as log;
use anyhow::{anyhow, Result};
use lyon_geom::{LineSegment, Point};
use lyon_path::iterator::Flattened;
use lyon_path::Path;
use lyon_svg::path_utils;
use roxmltree::{Document, NodeType};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Args {
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    #[structopt(short, long, parse(from_os_str), default_value="out.wav")]
    output: PathBuf,
}

const F_s: u32 = 44_100; // Hz
const DRAW_VELOCITY: f32 = 10000.; // units/s
const TOLERANCE: f32 = 0.01;

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

    let doc_txt = fs::read_to_string(args.input)?;
    let doc = Document::parse(&doc_txt)?;

    let mut pts: Vec<Point<f32>> = vec![];

    for node in doc.descendants() {
        if node.node_type() != NodeType::Element {
            // log::trace!("non-element: {:?}", node);
            continue;
        }

        if node.tag_name().name() == "path" {
            if let Some(d) = node.attribute("d") {
                let path = path_utils::build_path(Path::builder().with_svg(), d).unwrap();
                log::trace!("parsed Lyon path {:?}", path);

                let flattened = Flattened::new(TOLERANCE, path.iter());
                for evt in flattened {
                    log::trace!(" -> {:?}", evt);
                    match evt {
                        lyon_path::Event::Begin { at } => {
                            // slew at full speed to point
                            pts.push(at);
                        },
                        lyon_path::Event::End { last, first, close } => {
                            if close {
                                let line = LineSegment { from: last, to: first };
                                draw_line(&mut pts, line);
                                // emit_pos(&mut wtr, first)?; // loop back if closed. TODO: slew speed.
                            }
                        },
                        lyon_path::Event::Line { from, to } => {
                            // emit_pos(&mut wtr, to)?; // go where we're supposed to. TODO: slew speed.
                            let line = LineSegment { from, to };
                            draw_line(&mut pts, line);
                        },
                        _ => {
                            log::warn!("unsupported path element {:?}", evt);
                        }
                    }
                }
            } else {
                log::warn!("path node {:?} without actual path data", node);
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


    for _ in 0..1000 {
        for pt in &pts {
            wtr.write_sample(pt.x)?;
            wtr.write_sample(pt.y)?;
        }
    }

    wtr.finalize()?;

    Ok(())
}
